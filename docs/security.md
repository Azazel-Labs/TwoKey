# Security model

[Back to README](../README.md)

## Threat model and hardening notes

Untagged changes default to AI. The configured `[ai].human_marker` is a self-declaration,
not identity verification: an agent can forge it. Explicit AI signals still win.
GitHub review requirements remain separate from attribution.

Things twokey does on purpose:

- **Policy from the base revision**, never from the PR.
- **twokey's own config is always off limits to AI**, by built-in rules no policy
  can remove (human changes are governed by repository rules):
  - `twokey-policy`: every `.twokey.toml` anywhere, included sub-policies,
    `.twokey/**`, the CI runner `.github/twokey/**`, plus `settings.protect`.
  - `twokey-wiring`: any CI, git-hook or agent config file (`.github/workflows`,
    `.github/actions`, `.claude`, `.githooks`, `.husky`, pre-commit, lefthook,
    GitLab, CircleCI, Buildkite, Jenkins, Azure, Bitbucket) that mentions twokey
    before or after the change, so removing the twokey step is caught too.
  - The agent hook also refuses edits to user-level `~/.claude` settings that
    register twokey.
- **Unknown config keys, bad globs, regexes or queries are errors** (exit 2) so a
  typo can't silently disable a rule.
- Diffs use `--raw --no-renames --no-ext-diff --no-textconv`, content comes from
  blob ids, so repo config can't change what is compared. Renames are a delete
  plus an add, so moving a protected file out of a protected directory is caught.
- **Fail closed** when a protected file can't be verified: head has new syntax
  errors, file over `max_parse_bytes`, unterminated markers.
- Markers are read from the base, so deleting markers is itself a change to the
  protected region.
- Fingerprints are **structural**: whitespace is ignored but the syntax tree
  isn't, so dedenting a Python `return` out of an `if`, or a line break that
  triggers JS semicolon insertion, is a change.
- "Binary" never switches checks off (a NUL byte in a comment doesn't disable
  symbol, content or marker rules); it only adds a `binary` finding.
- Marker regions are located through the base→head line diff, so a decoy copy
  of a region elsewhere can't stand in for the real one, and lines inserted next
  to a region that would comment it out are flagged.
- A second definition of a protected name (shadowing) counts as a modification.
- Symlinks are resolved and evaluated against the rules protecting their target
  (file or directory), in CI and in the hook; case-insensitive globs by default.
- The hook denies edits inside any `.git` directory, finds the repo from the
  file being edited, prefers the policy at `settings.default_base`, and on
  `git commit` checks the index as well as the working tree.

Things you still need around it:

- **Run the CI check from trusted workflow code.** The included workflow uses
  `pull_request_target` so the workflow file comes from the base branch and
  installs twokey from a pinned release; it never executes PR code. Combine with
  branch protection, required status checks, CODEOWNERS on `.github/`, and
  any human-review requirements you choose in GitHub. Reviews do not bypass a
  twokey denial.
- **Give the agent a token that can't change rulesets, workflows or secrets, and
  can't approve PRs.**
- The hook's Bash check runs *after* the command (including failed commands,
  via `PostToolUseFailure`), so it can tell the agent to revert but can't
  prevent the write. It can't see what a background command writes later, or
  anything outside the working tree. The hook is for fast feedback; CI is the
  enforcement.
- Text-level tricks across files (e.g. moving logic out of a protected function
  into an unprotected helper it calls) need policy design: protect the module,
  not just the entry point, or use `symbols = ["*"]` for sensitive files.
