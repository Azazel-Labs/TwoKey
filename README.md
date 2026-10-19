# twokey

Programmatic guardrails for AI-authored changes. `twokey` looks at a change
(a PR's commit range, the staged index, or an agent's pending edit) and decides,
per your policy, whether it **passes**, **needs a named human's approval**, or is
**blocked** because AI-authored changes may never touch that code.

It's deterministic: globs, tree-sitter syntax trees, in-file markers, regexes and
size limits. No model in the loop.

The name comes from the two-person rule: like a launch that needs two keys
turned, an AI change to sensitive code needs a second, human key.

```
twokey: BLOCKED — 1 blocked, 1 awaiting approval, 0 approved, 0 warnings
  4 files, 37 lines changed · 2 of 3 commits AI-attributed
  policy: .twokey.toml @ 1a2b3c4d5e (origin/main)

  DENY      [ci-and-review-routing] .github/workflows/ci.yml — modified protected path
            CI and review routing are part of the enforcement layer; agents may not change them.
  APPROVE   [auth-logic] src/auth/mod.rs:3 — modified protected fn `Session::verify`
            Authentication/authorization logic.
            needs approval from one of: carol, dave
```

## What it can check

| Kind | Protects | Example |
|---|---|---|
| `path` | files by gitignore-style glob, per change type (added/modified/deleted) | workflows, CODEOWNERS, secrets, *applied* migrations (`on = ["modified","deleted"]`) |
| `symbol` | named functions/methods/types via tree-sitter, matched by qualified name (`auth::Session::verify`, `Session.verify`) | auth, crypto, billing entry points |
| `query` | any tree-sitter query; nodes captured as `@protect` are compared as a multiset | "no new `unsafe` blocks" |
| `content` | regex on added (or removed) lines | `#[ignore]`, `it.only(`, `eslint-disable`, removed `assert`s |
| markers | `twokey:lock-begin/end`, `twokey:review-begin/end`, `twokey:lock-file` in comments | a constants block compliance signed off on |
| limits | file/line/deletion counts, symlinks, submodules, mode changes, binaries | "AI PRs over 3000 lines need sign-off" |

Symbol and query comparisons are **structural**: whitespace is ignored, but every
token (comments included) and the shape of the syntax tree count. `rustfmt` on a
protected function passes; changing one token, or dedenting a Python `return`
out of an `if`, doesn't. Attributes, decorators and annotations are part of the
symbol, so removing `@require_auth` or adding `#[ignore]` counts. Renaming,
moving or redefining a protected symbol is caught.

Languages: Rust, Python, JavaScript, TypeScript, TSX, Go, Java, C#, Ruby (adding
a grammar is one match arm in `lang.rs` plus a definition table in `symbols.rs`).

## Where it runs

| Layer | Command | Role |
|---|---|---|
| Agent hook | `twokey hook claude` (PreToolUse + PostToolUse) | Simulates each Edit/Write in memory and answers `deny` / `ask` before the agent writes. Follows symlinks; refuses writes into `.git`; checks `git commit`/`git push`. After every Bash command it reports protected files the command itself changed and tells the agent to revert. **Seatbelt.** |
| Pre-commit | `twokey check --staged` | Checks the index. AI-ness from env vars / branch. `--no-verify` skips it. **Seatbelt.** |
| CI on the PR | `twokey check --base origin/main --head $SHA` | Policy from the target branch tip, diff from the merge-base, approvals from GitHub reviews of the exact head SHA. **The authority** — make it a required check. |

Exit codes: `0` pass, `1` blocked, `3` needs approval, `2` error (bad policy, git
failure). Errors never pass. In report-only mode (`--dry-run` or
`settings.enforce = false`) everything is still reported but the exit code is 0
and the agent hook stays silent.

## Maintainer

Nick Darnell is the sole maintainer.

## Rust compatibility

Building from source requires Rust 1.90 or newer and a C compiler for the
tree-sitter grammars. The project uses Rust 2024. CI tests the minimum Rust
version and current stable on Linux, macOS, and Windows. Keep `Cargo.lock`
committed and use `cargo build --locked` / `cargo test --locked` for reproducible
dependency selection.

## Install

Every release on GitHub carries prebuilt binaries (static Linux musl for x86_64
and aarch64, macOS arm64 and x86_64, Windows x86_64), a **vendored source bundle
that builds with no network access**, `SHA256SUMS`, and build-provenance
attestations. None of the options below except `cargo install` need crates.io.

| How | Command |
|---|---|
| Script (Linux/macOS) | `curl -fsSL https://github.com/YOUR_ORG/twokey/releases/download/v0.1.0/install.sh \| sh -s -- --version v0.1.0` |
| Pinned checksum | add `--sha256 <hex>` (from the release's `SHA256SUMS`) |
| GitHub Actions | `uses: YOUR_ORG/twokey@<commit-sha>` with `version:` and `sha256:` (see `action.yml`) |
| Direct download | `https://github.com/YOUR_ORG/twokey/releases/download/<tag>/twokey-<tag>-<target>.tar.gz` |
| Offline from source | download `twokey-<tag>-vendored-src.tar.gz`, then `cargo install --path <dir> --offline --locked` (or `install.sh --source`) |
| cargo-binstall | `cargo binstall twokey` (fetches the GitHub binary) |
| crates.io | `cargo install twokey --locked` |

Verify a download yourself with `sha256sum -c SHA256SUMS --ignore-missing` and
`gh attestation verify <file> --repo YOUR_ORG/twokey`. In CI, always pin both
the version and the sha256.

## Quick start

```sh
cd your-repo
twokey init                 # writes an example .twokey.toml
twokey validate --policy .twokey.toml   # compile globs, regexes, queries
twokey scan --policy .twokey.toml       # list exactly what is protected
```

Commit the policy, then wire up the layers:

- **CI:** see [GitHub setup](#github-setup).
- **Claude Code:** merge `integrations/claude/settings.json` into
  `.claude/settings.json` (PreToolUse for edits and git, PostToolUse for Bash).
- **Git hook:** `integrations/git/pre-commit` into `.githooks/`, then
  `git config core.hooksPath .githooks`.

Roll out in report-only mode (`enforce = false`, or the `TWOKEY_DRY_RUN`
repository variable in CI) for a week or two, read the PR summaries, tune the
policy, then enforce.

## GitHub setup

1. Copy `integrations/github/run.sh` to `.github/twokey/run.sh`, and
   `twokey.yml` + `twokey-review-trigger.yml` to `.github/workflows/`. Fill in
   the release URL and sha256 in `twokey.yml`.
2. **Create a GitHub App** with *Commit statuses: write*, *Pull requests: read*
   and, for team approvers, organization *Members: read*. Install it on the repo.
3. Create an environment named `twokey`, limited to your protected branches,
   holding variable `TWOKEY_APP_ID` and secret `TWOKEY_APP_KEY`.
4. Branch ruleset on `main`: require status check **`twokey/policy`** with the
   expected source set to **your App**; require PRs, dismiss stale approvals on
   push, CODEOWNERS on `.github/`, `.twokey.toml` and sub-policies.
5. Other protected bases (e.g. `release`): list them in `TWOKEY_BASES` on the
   twokey step and require **`twokey/policy (release)`** on that branch. The
   check only vouches for listed bases, and each base gets its own status name,
   because commit statuses belong to a commit: otherwise a second PR with the
   same head commit against a weaker branch could satisfy the check on `main`.
6. Pin the third-party actions in your copies to commit SHAs.

How it fits together:

- `twokey.yml` runs on `pull_request_target`, so the workflow, the runner script
  and the policy all come from the base branch. The PR is fetched as git data and
  never checked out or executed.
- Reviews trigger `twokey-review-trigger.yml`, a do-nothing job whose completion
  wakes `twokey.yml` through `workflow_run` (also base-branch code). Approving a
  PR re-runs the check automatically.
- `run.sh` re-reads the PR from the API, collects approvals of the exact head
  commit (only from owners, members and collaborators; never the author or
  bots), fetches team membership, runs `twokey check`, posts the commit status
  and keeps a single summary comment on the PR up to date. Any unexpected
  failure turns the status into `error`, never a stuck `pending`.
- Why the App: a status posted with the default Actions token can be imitated by
  any workflow a PR adds, under the same name. A required check pinned to your
  App can't, and the App key lives in an environment PR workflows can't use.

Without an App it still works (the status is posted with `github.token`); use an
organization ruleset's *required workflows* instead to make it binding.

## Deciding what's "AI"

Per commit in the range, from `[ai]`:

- trailers (`Co-authored-by: Claude …`, `Assisted-by:`, `Generated-by:`)
- author/committer identities (`noreply@anthropic.com`, `[bot]`)
- branch name (`claude/…`, `codex/…`, `agent/…`)
- env vars for local checks (`CLAUDECODE`, `TWOKEY_AI`)

A finding is AI-attributed if any AI commit in the range touched that file, so a
human commit in the same PR isn't blocked by an agent's commit elsewhere.

These signals are only as honest as the agent. To fail closed, use one of:

- `require_signed_human = true` (+ `human_signing_keys`): a commit is human only
  if it carries a good signature from a trusted key. Give agents no signing key.
- `assume_ai = true`: everything is AI; human-only lanes pass `--human`.

## Approvals

`approve` findings list who can approve: logins, `group:name` from
`[approvals.groups]`, or GitHub teams (`team:org/slug` / `@org/slug`), falling
back to `[approvals] approvers`, falling back to anyone. CI passes the logins
that approved **this exact head commit** via `--approver` / `TWOKEY_APPROVERS`,
and team membership via `--teams-file` (`twokey teams` lists which teams the
policy uses). The provided runner excludes the PR author and bot accounts.
`deny` findings can't be approved away: a human has to make that change in a
commit that isn't AI-attributed.

## Monorepos

`settings.include = ["services/*/.twokey.toml"]` pulls in per-directory
policies. A sub-policy may only contain `[[rule]]` tables; its paths are relative
to its directory (no `paths` = the whole directory), its rule ids are prefixed
with the directory, and it can't change attribution, approvals, limits or
anything else in the root policy. Rules only add findings, so a team can tighten
its own directory but never loosen anyone else's. Sub-policy files are protected
like the root policy.

## Threat model and hardening notes

Things twokey does on purpose:

- **Policy from the base revision**, never from the PR.
- **twokey's own config is always off limits to AI**, by built-in rules no policy
  can remove (humans need approval):
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
- Approver groups that resolve to nobody are an error, never "anyone".

Things you still need around it:

- **Run the CI check from trusted workflow code.** The included workflow uses
  `pull_request_target` so the workflow file comes from the base branch and
  installs twokey from a pinned release; it never executes PR code. Combine with
  branch protection, required status checks, CODEOWNERS on `.github/`, and
  "dismiss stale approvals on new commits".
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

## Policy reference

See `examples/twokey.toml` for a commented, complete policy. Rule fields:

```toml
[[rule]]
id = "unique-id"
kind = "path" | "symbol" | "query" | "content"
action = "deny" | "approve" | "warn" | "allow"   # for AI changes (default approve)
human_action = "allow"                           # for human changes (default allow)
paths = ["/src/auth/**", "*.pem"]                # leading / anchors; no slash = any depth
exclude = ["**/fixtures/**"]
on = ["added", "modified", "deleted"]            # default: all
approvers = ["group:security", "alice"]
reason = "shown to reviewers and to the agent"
# symbol:  symbols = ["verify_*", "Session::*"]   language = "rust" (optional)
# query:   language = "rust"  query = "(unsafe_block) @protect"  capture = "protect"
# content: pattern = '#\[ignore\]'  lines = "added" | "removed" | "both"
```

## Publishing (maintainers)

1. Make sure `version` in `Cargo.toml` matches the tag you'll push, and that
   `Cargo.lock` is committed.
2. Push the tag (`git tag v0.1.0 && git push origin v0.1.0`). The release
   workflow tests and builds every target, vendors the source and proves it
   builds offline, writes `SHA256SUMS`, attests provenance, and creates the
   GitHub release.
3. crates.io, first time only: `cargo login` with an API token, then
   `cargo publish --locked` from the tag. Then, on crates.io → the crate →
   Settings → Trusted Publishing, add this repository, workflow `release.yml`
   and environment `crates-io`, create that environment in GitHub (require a
   reviewer if you like), set the repository variable `CRATES_IO_TRUSTED=true`,
   and revoke the API token. From then on every tag publishes to crates.io from
   CI with a short-lived OIDC token; no secret is stored anywhere.

## Development

```sh
cargo test                 # unit tests (engine, symbols, markers, config, hook, report)
cargo build && tests/e2e.sh target/debug/twokey   # 46 scenarios on a scratch repo
```

Commit `Cargo.lock` after the first build: this repo's own CI and release
workflows build with `--locked`. Pushing a `v*` tag builds Linux (x86_64,
aarch64), macOS (arm64) and Windows binaries with SHA-256 sums and build
provenance, and publishes a GitHub release.


