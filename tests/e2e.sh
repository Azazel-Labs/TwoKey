#!/usr/bin/env bash
# End-to-end scenarios against a throwaway git repo.
# Usage: tests/e2e.sh [path/to/twokey]   (default: target/debug/twokey)
set -euo pipefail

BIN="$(cd "$(dirname "${1:-target/debug/twokey}")" && pwd)/$(basename "${1:-target/debug/twokey}")"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

pass=0; fail=0
expect() { # expect <exit-code> <description> -- cmd...
  local want=$1 desc=$2; shift 3
  set +e; out=$("$@" 2>&1); got=$?; set -e
  if [[ $got == "$want" ]]; then pass=$((pass+1)); echo "ok    $desc"
  else fail=$((fail+1)); echo "FAIL  $desc (exit $got, wanted $want)"; echo "$out" | sed 's/^/      /'; fi
}

git init -q -b main repo && cd repo
git config core.autocrlf false
git config core.symlinks true
HOOK_CWD="$PWD"
case "$(uname -s)" in
  MINGW*|MSYS*)
    export MSYS=winsymlinks:nativestrict
    HOOK_CWD="$(pwd -W)"
    ;;
esac
git config user.name "Human Dev"; git config user.email dev@example.com
unset CLAUDECODE TWOKEY_AI TWOKEY_BRANCH || true
command -v gsed >/dev/null && sed() { gsed "$@"; }   # macOS: GNU sed needed for -i

# Exercise the opt-in samples; the shipped starter keeps them disabled.
sed '/^# \[\[rule\]\]/,$s/^# //' "$ROOT/examples/twokey.toml" > .twokey.toml
# Fix the fixture declaration independently of the starter's chosen marker.
sed -i 's/^human_marker = .*/human_marker = "Human-Authored: true"/' .twokey.toml
mkdir -p src/auth src/crypto .github/workflows migrations tests
cat > src/auth/mod.rs <<'EOF'
pub struct Session { token: String }
impl Session {
    pub fn verify(&self) -> bool { self.token.len() == 32 }
}
pub fn verify_token(t: &str) -> bool { t.starts_with("tok_") && t.len() == 36 }
pub fn greeting() -> &'static str { "hello" }
EOF
cat > src/lib.rs <<'EOF'
pub mod auth;
// twokey:lock-begin limits agreed with compliance
pub const MAX_TRANSFER: u64 = 10_000;
// twokey:lock-end
pub fn add(a: u64, b: u64) -> u64 { a + b }
#[cfg(test)]
mod tests {
    #[test]
    fn adds() { assert_eq!(super::add(1, 2), 3); }
}
EOF
echo 'fn k() -> u8 { 7 }' > src/crypto/k.rs
echo 'name: ci' > .github/workflows/ci.yml
echo 'CREATE TABLE t (id int);' > migrations/001_init.sql
git add -A && git commit -qm "initial"

ai_commit() { git add -A && git commit -qm "$1"; }
check() { "$BIN" check --base main --head HEAD "$@"; }
branch() { git checkout -qb "$1" main; }

expect 0 "policy validates" -- "$BIN" validate

branch ai/harmless
sed -i 's/"hello"/"hello, world"/' src/auth/mod.rs
ai_commit "tweak greeting"
expect 0 "AI edit to unprotected function passes" -- check

branch ai/auth
sed -i 's/self.token.len() == 32/true/' src/auth/mod.rs
ai_commit "simplify verify"
expect 1 "AI edit to Session::verify is denied" -- check

branch ai/rename
sed -i 's/pub fn verify_token/pub fn check_token/' src/auth/mod.rs
ai_commit "rename"
expect 1 "renaming a protected fn is caught" -- check

branch ai/fmt
python3 - <<'EOF'
p='src/auth/mod.rs'; s=open(p).read()
s=s.replace('pub fn verify(&self) -> bool { self.token.len() == 32 }','pub fn verify(&self) -> bool {\n        self.token.len() == 32\n    }')
open(p,'w').write(s)
EOF
ai_commit "rustfmt"
expect 0 "pure reformat of protected fn passes" -- check

branch ai/workflow
echo '  # sneaky' >> .github/workflows/ci.yml
ai_commit "ci tweak"
expect 1 "AI edit to workflow is denied" -- check

branch human/workflow
echo '  # human' >> .github/workflows/ci.yml
git add -A && git commit -qm "human ci change" -m "Human-Authored: true"
expect 0 "human workflow change passes" -- check

branch feature/untagged
sed -i 's/self.token.len() == 32/true/' src/auth/mod.rs
git add -A && git commit -qm "local model without attribution"
expect 1 "untagged commit on ordinary branch is AI" -- check
echo '// human follow-up' >> src/auth/mod.rs
git add -A && git commit -qm "human follow-up" -m "Human-Authored: true"
expect 1 "human follow-up cannot clear earlier AI edits" -- check

branch ai/human-claim
sed -i 's/self.token.len() == 32/true/' src/auth/mod.rs
git add -A && git commit -qm "human claim on agent branch" -m "Human-Authored: true"
expect 1 "agent branch overrides human declaration" -- check

branch ai/policy
sed -i 's/action = "deny"/action = "allow"/' .twokey.toml
ai_commit "relax policy"
expect 1 "AI can't relax the policy" -- check

branch ai/lock
sed -i 's/10_000/1_000_000/' src/lib.rs
ai_commit "raise limit"
expect 1 "lock-marked region is denied" -- check

branch ai/unlock
sed -i '/twokey:lock-/d' src/lib.rs
ai_commit "remove markers"
expect 1 "removing lock markers is denied" -- check

branch ai/test
sed -i 's/assert_eq!(super::add(1, 2), 3);/assert!(true);/' src/lib.rs
ai_commit "fix test"
expect 1 "rewriting an existing test is denied" -- check

branch ai/skip
sed -i 's/#\[test\]/#[test]\n    #[ignore]/' src/lib.rs
ai_commit "skip flaky"
expect 1 "adding #[ignore] is denied" -- check

branch ai/unsafe
echo 'pub fn raw(p: *const u8) -> u8 { unsafe { *p } }' >> src/lib.rs
ai_commit "fast path"
expect 1 "new unsafe block is denied" -- check

branch ai/migration
echo 'ALTER TABLE t ADD x int;' >> migrations/001_init.sql
ai_commit "edit migration"
expect 1 "editing an applied migration is denied" -- check

branch ai/newmigration
echo 'ALTER TABLE t ADD x int;' > migrations/002_x.sql
ai_commit "new migration"
expect 0 "adding a new migration is fine" -- check

branch ai/symlink
# Build the Git symlink entry directly, including on hosts without symlink privileges.
link_blob=$(printf '%s' '../.github/workflows/ci.yml' | git hash-object -w --stdin)
git update-index --add --cacheinfo "120000,$link_blob,src/ci-link.yml"
git commit -qm "link"
expect 1 "symlink into protected path is denied" -- check

branch ai/crypto
echo 'use std::fmt;' >> src/crypto/k.rs
ai_commit "touch crypto"
expect 1 "any AI change in the crypto module is denied" -- check

branch ai/nul-byte
printf '// \000 harmless\n' | cat - src/auth/mod.rs > t && mv t src/auth/mod.rs
sed -i 's/self.token.len() == 32/true/' src/auth/mod.rs
ai_commit "nul byte trick"
expect 1 "a NUL byte doesn't switch off symbol checks" -- check

branch claude/no-trailer
sed -i 's/self.token.len() == 32/true/' src/auth/mod.rs
git add -A && git commit -qm "no trailer, but agent branch"
expect 1 "agent branch name marks change as AI" -- check --branch claude/no-trailer
expect 0 "--human override (trusted CI lanes only)" -- check --human

# Built-in protection of twokey's own wiring, even with an empty policy
: > "$WORK/empty.toml"
branch human/husky
mkdir -p .husky && printf '#!/bin/sh\ntwokey check --staged\n' > .husky/pre-commit
git add -A && git commit -qm "add twokey hook"
git checkout -qb ai/unhook
printf '#!/bin/sh\nexit 0\n' > .husky/pre-commit
ai_commit "speed up commits"
expect 1 "removing twokey from a git hook is denied, even with an empty policy" -- \
  "$BIN" check --base human/husky --head HEAD --policy "$WORK/empty.toml"

git checkout -q main
expect 0 "human-only range passes" -- check

# staged (pre-commit) mode
echo '  # staged' >> .github/workflows/ci.yml && git add -A
expect 1 "untagged staged changes default to AI" -- "$BIN" check --staged
printf 'Human change\n\nHuman-Authored: true\n\n# Git editor instructions\n' > "$WORK/message"
expect 0 "staged human declaration passes" -- "$BIN" check --staged --commit-message-file "$WORK/message"
expect 1 "AI environment overrides human declaration" -- env CLAUDECODE=1 "$BIN" check --staged --commit-message-file "$WORK/message"
printf 'Mixed change\n\nGenerated-by: local-model\nHuman-Authored: true\n' > "$WORK/message"
expect 1 "AI message overrides staged human declaration" -- "$BIN" check --staged --commit-message-file "$WORK/message"
expect 2 "missing commit message is an error" -- "$BIN" check --staged --commit-message-file "$WORK/missing-message"
expect 1 "staged change with CLAUDECODE set is denied" -- env CLAUDECODE=1 "$BIN" check --staged
git checkout -q -- . && git reset -q --hard

# agent hook
hook() { printf '%s' "$1" | "$BIN" hook claude; }
out=$(hook "{\"tool_name\":\"Edit\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"file_path\":\"$HOOK_CWD/.github/workflows/ci.yml\",\"old_string\":\"ci\",\"new_string\":\"x\"}}")
[[ $out == *'"deny"'* ]] && { pass=$((pass+1)); echo "ok    hook denies workflow edit"; } || { fail=$((fail+1)); echo "FAIL  hook workflow: $out"; }
out=$(hook "{\"tool_name\":\"Edit\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"file_path\":\"$HOOK_CWD/src/auth/mod.rs\",\"old_string\":\"self.token.len() == 32\",\"new_string\":\"true\"}}")
[[ $out == *'"deny"'* ]] && { pass=$((pass+1)); echo "ok    hook denies protected fn edit"; } || { fail=$((fail+1)); echo "FAIL  hook protected edit: $out"; }
out=$(hook "{\"tool_name\":\"Edit\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"file_path\":\"$HOOK_CWD/src/auth/mod.rs\",\"old_string\":\"\\\"hello\\\"\",\"new_string\":\"\\\"hi\\\"\"}}")
[[ -z $out ]] && { pass=$((pass+1)); echo "ok    hook passes unprotected edit"; } || { fail=$((fail+1)); echo "FAIL  hook pass: $out"; }
out=$(hook "{\"tool_name\":\"Write\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"file_path\":\"$HOOK_CWD/.twokey.toml\",\"content\":\"\"}}")
[[ $out == *'"deny"'* ]] && { pass=$((pass+1)); echo "ok    hook denies policy overwrite"; } || { fail=$((fail+1)); echo "FAIL  hook policy: $out"; }
if ln -s .github/workflows/ci.yml innocent.yml; then
out=$(hook "{\"tool_name\":\"Write\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"file_path\":\"$HOOK_CWD/innocent.yml\",\"content\":\"pwned\"}}")
[[ $out == *'"deny"'* ]] && { pass=$((pass+1)); echo "ok    hook follows symlinks"; } || { fail=$((fail+1)); echo "FAIL  hook symlink: $out"; }
rm innocent.yml
elif [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* ]]; then
  echo "SKIP  hook follows symlinks (Windows symlink privilege unavailable)"
else
  echo "FAIL  cannot create hook symlink fixture"
  fail=$((fail+1))
fi
out=$(hook "{\"tool_name\":\"Write\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"file_path\":\"$HOOK_CWD/.git/hooks/pre-commit\",\"content\":\"exit 0\"}}")
[[ $out == *'"deny"'* ]] && { pass=$((pass+1)); echo "ok    hook denies writes into .git"; } || { fail=$((fail+1)); echo "FAIL  hook .git: $out"; }
echo '# x' >> .github/workflows/ci.yml && git add -A && git show HEAD:.github/workflows/ci.yml > .github/workflows/ci.yml
out=$(hook "{\"tool_name\":\"Bash\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"command\":\"git commit -m staged-only\"}}")
[[ $out == *'"deny"'* ]] && { pass=$((pass+1)); echo "ok    hook sees staged-only changes"; } || { fail=$((fail+1)); echo "FAIL  hook staged: $out"; }
git reset -q --hard
echo '# x' >> .github/workflows/ci.yml
out=$(hook "{\"tool_name\":\"Bash\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"command\":\"git commit -am wip\"}}")
[[ $out == *'"deny"'* ]] && { pass=$((pass+1)); echo "ok    hook checks git commit"; } || { fail=$((fail+1)); echo "FAIL  hook commit: $out"; }
git checkout -q -- .

# PostToolUse on Bash: only changes made by *this* command are reported
bash_hook() { hook "{\"hook_event_name\":\"$1\",\"tool_name\":\"Bash\",\"tool_use_id\":\"$2\",\"cwd\":\"$HOOK_CWD\",\"tool_input\":{\"command\":\"$3\"}}"; }
bash_hook PreToolUse t1 "sed -i x ci.yml" >/dev/null
echo '# via sed' >> .github/workflows/ci.yml
out=$(bash_hook PostToolUse t1 "sed -i x ci.yml")
[[ $out == *'"block"'* ]] && { pass=$((pass+1)); echo "ok    post-Bash check catches direct file writes"; } || { fail=$((fail+1)); echo "FAIL  post bash: $out"; }
bash_hook PreToolUse t2 "ls" >/dev/null
out=$(bash_hook PostToolUse t2 "ls")
[[ -z $out ]] && { pass=$((pass+1)); echo "ok    post-Bash check ignores pre-existing changes"; } || { fail=$((fail+1)); echo "FAIL  post bash noise: $out"; }
git checkout -q -- .
bash_hook PreToolUse t3 "sed -i x ci.yml; false" >/dev/null
echo '# via failing cmd' >> .github/workflows/ci.yml
out=$(bash_hook PostToolUseFailure t3 "sed -i x ci.yml; false")
[[ $out == *'"block"'* ]] && { pass=$((pass+1)); echo "ok    failed Bash commands are checked too"; } || { fail=$((fail+1)); echo "FAIL  post failure: $out"; }
bash_hook PreToolUse t4 "sed -i y ci.yml" >/dev/null
echo '# edited again' >> .github/workflows/ci.yml
out=$(bash_hook PostToolUse t4 "sed -i y ci.yml")
[[ $out == *'"block"'* ]] && { pass=$((pass+1)); echo "ok    re-editing an already dirty protected file is caught"; } || { fail=$((fail+1)); echo "FAIL  post re-edit: $out"; }
git checkout -q -- .

# report-only mode and PR summary
git checkout -q ai/workflow
expect 0 "--dry-run reports but doesn't fail" -- check --dry-run
check --summary-out "$WORK/summary.md" >/dev/null || true
grep -q 'twokey-report' "$WORK/summary.md" && grep -q 'ci-and-review-routing' "$WORK/summary.md" \
  && { pass=$((pass+1)); echo "ok    markdown summary written"; } || { fail=$((fail+1)); echo "FAIL  summary"; }

# Removed approval commands and options cannot bypass denials.
expect 2 "approver option is rejected" -- check --approver alice
expect 2 "team command is rejected" -- "$BIN" teams

git checkout -q main

# Run the actual recommended commit-msg hook, including its message-file argument.
export PATH="$(dirname "$BIN"):$PATH"
cp "$ROOT/integrations/git/commit-msg" .git/hooks/commit-msg
chmod +x .git/hooks/commit-msg
echo '# hook check' >> .github/workflows/ci.yml
git add -A
expect 1 "commit-msg hook rejects untagged protected change" -- git commit -qm "untagged"
expect 0 "commit-msg hook accepts fully human declaration" -- git commit -qm "human change" -m "Human-Authored: true"

# The committed policy chooses the declaration, including literal # markers.
rm .git/hooks/commit-msg
sed -i 's/human_marker = "Human-Authored: true"/human_marker = "#Human"/' .twokey.toml
git add .twokey.toml && git commit -qm "choose human marker"
git checkout -qb feature/custom-human
cp "$ROOT/integrations/git/commit-msg" .git/hooks/commit-msg
chmod +x .git/hooks/commit-msg
echo '# custom marker' >> .github/workflows/ci.yml
git add -A
expect 1 "old marker no longer declares human" -- git commit -qm "human change" -m "Human-Authored: true"
expect 0 "commit-msg hook accepts configured hash marker" -- git commit -qm "human change" -m "#Human"
expect 0 "CI recognizes stored custom marker" -- check
echo '# editor marker' >> .github/workflows/ci.yml
git add -A
expect 0 "custom marker survives editor-style cleanup" -- git -c core.commentChar=';' commit --cleanup=strip -qm "human change" -m "#Human"
expect 0 "CI recognizes marker after editor-style cleanup" -- check

echo; echo "$pass passed, $fail failed"
[[ $fail == 0 ]]
