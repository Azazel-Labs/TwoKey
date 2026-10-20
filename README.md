# twokey

**Let AI write code. Decide what it cannot change.**

[![CI](https://github.com/Azazel-Labs/twokey/actions/workflows/ci.yml/badge.svg)](https://github.com/Azazel-Labs/twokey/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Azazel-Labs/twokey)](https://github.com/Azazel-Labs/twokey/releases)
[![crates.io](https://img.shields.io/crates/v/twokey.svg)](https://crates.io/crates/twokey)
[![License: CC0-1.0](https://img.shields.io/badge/license-CC0--1.0-blue.svg)](LICENSE)
[![Rust: 1.90+](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](Cargo.toml)
[![Edition: 2024](https://img.shields.io/badge/edition-2024-orange.svg)](Cargo.toml)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)](#installation)

English · [Español](docs/i18n/README.es.md) · [Français](docs/i18n/README.fr.md) · [Deutsch](docs/i18n/README.de.md) · [日本語](docs/i18n/README.ja.md) · [简体中文](docs/i18n/README.zh-CN.md)

[Use cases](#what-would-you-protect) · [Quick start](#quick-start) · [Installation](#installation) · [Configuration](docs/configuration.md) · [GitHub Actions](docs/github-actions.md) · [Security model](docs/security.md) · [Contributing](#contributing)

An agent fixes a bug by weakening the test. A refactor touches token validation.
A quick cleanup edits a migration that already ran in production.

**twokey turns "don't touch that" into a repository policy you can check.** Give
AI room to work while keeping your authentication logic, existing tests, release
workflows, or a few critical lines off limits. Choose the boundaries in
`.twokey.toml`, get feedback locally, and require a passing check before merging.

- **Protect a function without freezing the file.** Tree-sitter rules identify
  named functions and methods, so nearby code can still change.
- **Keep the tests that define correct behavior.** Allow new tests while blocking
  AI edits to existing ones, or catch newly added test skips.
- **Make the boundary survive the PR.** CI reads the policy from the target
  branch, so a PR cannot relax its own checks.
- **Use the model you want.** Untagged changes default to AI, including changes
  from local models that leave no attribution metadata.

Checks run locally with file patterns, syntax trees, markers, and regexes.
No model calls, API keys, or code uploads to an AI service. GitHub continues to
handle reviews and merge permissions.

## What would you protect?

| Let AI work on… | Keep it away from… | Policy to use |
| --- | --- | --- |
| Login UI and error messages | Token validation and permission checks | Named symbols |
| A bug fix and new regression tests | Rewriting existing tests to make the fix pass | Symbols with `on = ["modified", "deleted"]` |
| A new database migration | Editing migrations already applied in production | Paths with `on = ["modified", "deleted"]` |
| Application features | CI, release workflows, dependency files, or cryptography | File and directory paths |
| The surrounding implementation | A reviewed calculation or fixed protocol constant | `twokey:lock` markers |
| Documentation | Em dashes or wording you do not want added | Content regexes |

These are choices you make for your repository. The starter's example rules are
commented out; enable the ones you want and adapt their paths.

### Keep authentication logic human-written

Add this rule to `.twokey.toml`:

```toml
[[rule]]
id = "token-validation"
kind = "symbol"
paths = ["/src/auth/**"]
symbols = ["verify_token", "Session::verify"]
action = "deny"
reason = "Token validation must be changed by a human."
```

Inside `impl Session`, an AI change like this is blocked:

```diff
 pub fn verify(&self) -> bool {
-    self.token.len() == 32
+    true
 }
```

An edit to an unprotected `greeting()` function in the same file can pass this
rule. Renaming or deleting the protected function is caught too. For code whose
imports and helpers also need protection, use a path rule for the entire module.

### Let AI add tests without rewriting existing ones

```toml
[[rule]]
id = "existing-rust-tests"
kind = "symbol"
paths = ["**/*.rs"]
symbols = ["tests::*"]
on = ["modified", "deleted"]
action = "deny"
reason = "Add regression tests; preserve existing test behavior."
```

This protects existing functions in Rust `tests` modules. Adding a new test is
allowed by this rule; changing an existing assertion or deleting a test is not.
The [starter policy](examples/twokey.toml) also has examples for Python and Go
tests, test skips, and removed assertions.

### Lock a few critical lines

Put markers around a region you want AI to leave alone:

```rust
// twokey:lock-begin transfer limit agreed with compliance
pub const MAX_TRANSFER: u64 = 10_000;
// twokey:lock-end
```

Markers are enabled by default and read from the base revision. Changing the
constant or deleting the markers is blocked for AI. No separate path rule is
needed for this region.

### Keep writing conventions out of review comments

```toml
[[rule]]
id = "no-em-dashes"
kind = "content"
paths = ["*.md"]
pattern = '\x{2014}'
lines = "added"
action = "deny"
human_action = "deny"
reason = "Use a comma, colon, parentheses, or a separate sentence."
```

This rejects em dashes on added or replacement Markdown lines, including human
changes. Existing untouched lines are left alone. Omit `human_action` to apply
it only to AI changes, or adapt the regex to catch unwanted wording.

Syntax-aware rules support **Rust, Python, JavaScript, TypeScript/TSX, Go, Java,
C#, and Ruby**. Path and content rules work with other file types too.
See the [full rule reference](docs/configuration.md#policy-reference) for more.

## Quick start

[Install the CLI](#installation), then run this inside the Git repository you
want to protect:

```sh
twokey init
```

Open `.twokey.toml`, uncomment the complete `[[rule]]` blocks you want, and adapt
their paths. The [example policy](examples/twokey.toml) includes file, symbol,
content, and syntax rules, all commented out by default. Optional size limits
are also commented out. Built-in policy/wiring protection, source markers, and
default symlink/submodule/mode-change protections remain enabled.

Validate and preview your choices:

```sh
twokey validate --policy .twokey.toml
twokey scan --policy .twokey.toml
twokey check --staged --policy .twokey.toml --dry-run
```

`--dry-run` reports without blocking. `scan` examines committed files at `HEAD`,
so the repository needs a commit. Commit the policy before enabling enforcement.

### Declare a fully human change

Choose the human declaration in `.twokey.toml`:

```toml
[ai]
human_marker = "#Human" # selected by twokey init; choose your own marker
```

**Changes are assumed to involve AI by default**, even when the model leaves no
attribution tags. For a change written entirely by a human, add this trailer at
the end of the commit message, separated from the subject/body by a blank line:

```text
Fix token validation

#Human
```

With the starter policy:

```sh
git commit -m "Fix token validation" -m "#Human"
```

The marker matches a whole line, case-insensitively, in the final paragraph.
You can choose another literal marker, such as `Human-Authored: true`. With a
`#` marker, configure
`git config core.commentChar ';'` when using Git's editor, so Git keeps the marker
instead of treating it as a comment.

Use the declaration only for fully human work, including the initial policy setup
when written by a human. AI-assisted or mixed work keeps the default AI treatment.
AI author/message signals and agent branch names still take precedence. A human
commit cannot clear AI attribution from an earlier commit touching the same file
in the PR. Preserve declarations when rewriting or squashing commits.

This is a declaration, not proof of authorship; see [attribution configuration](docs/configuration.md#deciding-whats-ai).

## Installation

Version 0.2.0 introduces the approval-free enforcement model. Upgrading from
v0.1.2 or earlier requires policy changes; see the
[migration notes](docs/configuration.md#migrating-from-the-approval-model).

Use the **GitHub Action for CI** or **Cargo for local development**.

### GitHub Action (recommended for CI)

Add these steps to your workflow to install a prebuilt binary without a Rust
toolchain or compilation:

```yaml
- name: Install twokey
  uses: Azazel-Labs/TwoKey@6ebf78b9bdcfa412df2b505fea00a6f4bd495a77 # v0.1.1
  with:
    version: v0.2.0

- run: twokey --version
```

The [Install twokey action](action.yml) verifies the release checksum and adds
`twokey` to `PATH` for subsequent steps. It supports the Linux, macOS, and Windows
platforms listed below. The `uses` SHA pins the installer code; `version` selects
the CLI release. For a pinned archive checksum too, set the `sha256` input to the
matching platform's entry in the release's
[SHA256SUMS](https://github.com/Azazel-Labs/TwoKey/releases/download/v0.2.0/SHA256SUMS).

To **enforce policy on pull requests**, follow the [GitHub Actions setup guide](docs/github-actions.md).
It supplies the trusted workflow and runner that read policy from the base
revision, check AI-attributed changes, and publish the
`twokey/policy` status to require in branch protection. The install step alone
does not run policy checks. The integration can use `GITHUB_TOKEN`; a dedicated
GitHub App is optional and lets branch protection bind results to that App.

### Cargo (recommended for local use)

Install the published CLI from [crates.io](https://crates.io/crates/twokey):

```sh
cargo install twokey --locked
twokey --version
```

To select a specific release, use `cargo install twokey --version 0.2.0 --locked`.
Cargo builds the CLI locally, requiring **Rust 1.90 or newer** and a C compiler
for the tree-sitter grammars. On Windows, use MSVC with Visual Studio C++ Build
Tools. Git is required when using twokey. Continue with the [quick start](#quick-start)
to configure and check your repository.

### Prebuilt binaries without Cargo

**Linux / macOS:**

```sh
curl -fsSL https://raw.githubusercontent.com/Azazel-Labs/twokey/main/install.sh | sh
```

**Windows (PowerShell 5.1 or newer):**

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/Azazel-Labs/twokey/main/install.ps1 -OutFile install.ps1
./install.ps1
```

Both installers verify the archive against the release's `SHA256SUMS` and default
to `~/.local/bin`; add that directory to your `PATH`. Windows uses the x64 binary
(ARM64 requires Windows x64 emulation). Use `-Version v0.2.0`, `-To <directory>`,
and `-Sha256 <hex>` in PowerShell, or `--version`, `--to`, and `--sha256` in the
shell installer, to select a release, destination, and pinned checksum. Neither
installer needs administrator privileges when using the default directory.

### Build from a checkout

For development, build and install from source using the same prerequisites as Cargo above:

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

[GitHub Releases](https://github.com/Azazel-Labs/twokey/releases) provide Linux
(x86_64 and aarch64 musl), macOS (Intel and Apple Silicon), and Windows (x86_64)
binaries, an offline source bundle, checksums, and provenance attestations.
See the [release guide](docs/releasing.md) for distribution and verification details.

## Require twokey before merging on GitHub

Use a required status check to make twokey a prerequisite for merging into
`main`. The supplied integration checks a PR when it opens and checks again
whenever new commits are pushed to that PR (`synchronize`). Reviews are handled
by GitHub and do not override a policy violation.

1. Add and commit `.twokey.toml` using the quick start above.
2. Copy these integration files into the repository you want to protect:

   | Source | Destination |
   | --- | --- |
   | [run.sh](integrations/github/run.sh) | `.github/twokey/run.sh` |
   | [twokey.yml](integrations/github/twokey.yml) | `.github/workflows/twokey.yml` |

3. In the copied `twokey.yml`, replace `COMMIT_SHA` with a released TwoKey commit
   SHA, set `version` to that CLI release, and fill in `sha256` from its
   `SHA256SUMS` entry for `x86_64-unknown-linux-musl` (the template uses Ubuntu).
   Create a GitHub environment named `twokey`. The basic setup uses the built-in
   `GITHUB_TOKEN`; leave `TWOKEY_APP_CLIENT_ID` unset to use it.
4. Merge this setup into the default branch and each protected PR base branch.
   Open a test PR to generate the `twokey/policy` status. Keep `TWOKEY_DRY_RUN`
   unset or `false` for enforcement.
5. In **Settings → Rules → Rulesets**, create an **Active** branch ruleset
   targeting `main`. Enable **Require a pull request before merging** and
   **Require status checks to pass**, then select **`twokey/policy`**. Leave the
   bypass list empty for anyone who must obey the policy, including agents.
   Keep force pushes blocked. Repeat for other protected branches using the
   status names described in the [setup guide](docs/github-actions.md).

An AI-attributed change to protected code cannot merge, even if the PR has
approving reviews. Remove the prohibited change from the AI-authored work. Protect the policy and
workflow files with human CODEOWNERS reviews. For protection against another
workflow posting a fake passing status, use the optional dedicated GitHub App
and select it as the status's expected source; see the [setup guide](docs/github-actions.md).

### Can twokey stop a commit or push?

GitHub Actions runs **after** a push or PR event reaches GitHub. It cannot run
before a PR is created or reject the initial upload to a feature branch. The
ruleset above rejects direct updates to protected `main` for users without a
bypass and prevents PR merges until checks pass. See GitHub's
[branch protection documentation](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).

For feedback **before a local commit**, install twokey and copy the supplied
[commit-msg hook](integrations/git/commit-msg) into `.githooks/commit-msg` in your
repository, then run:

```sh
git config core.hooksPath .githooks
```

On Linux or macOS, also run `chmod +x .githooks/commit-msg`. If you already have
hooks configured, integrate the check into those hooks instead of replacing them.
Each contributor must configure their own clone.

The supplied hook checks staged changes and the proposed commit message. It
blocks denials and errors, including a missing policy or twokey installation:

```sh
twokey check --staged --require-policy --commit-message-file "$1"
```

Use this hook instead of the older pre-commit hook if you want human declarations:
pre-commit runs before the message is available and assumes AI. Agent edit hooks
always treat edits as AI, regardless of any proposed commit message.

A local `pre-push` hook can also reject a push before upload, but must check the
commit ranges being pushed, not staged changes. No pre-push hook is bundled.
Local hooks can be skipped with `--no-verify` or removed, so keep GitHub's required
checks as the enforcement boundary.

## Usage

```sh
# Check staged changes using the committed policy.
twokey check --staged

# Evaluate a pull request using policy from its target branch.
twokey check --base origin/main --head HEAD --require-policy

# Produce structured output.
twokey check --staged --format json
```

Replace `origin/main` with your target branch. The supplied CI runner checks the
current PR head using policy from its target branch. twokey has no approval
commands, reviewer lists, or groups.

| Exit code | Meaning |
| --- | --- |
| `0` | Passed, or report-only mode |
| `1` | Blocked by policy |
| `2` | Error, including invalid policy |

### Integrations

| Integration | Setup | Purpose |
| --- | --- | --- |
| GitHub Actions | [Setup guide](docs/github-actions.md) | Enforce policy with a required status check |
| Git commit-msg | Copy [commit-msg](integrations/git/commit-msg) into `.githooks/`, then run `git config core.hooksPath .githooks` | Check staged edits and the human declaration locally |
| Claude Code | Merge [hook settings](integrations/claude/settings.json) into `.claude/settings.json` | Check edits and report protected changes after shell commands |

Local hooks can be bypassed. Trusted CI, branch protection, and restricted agent
credentials provide enforcement. The human marker is a declaration, not proof of who wrote the code. Read the
[security model](docs/security.md) before relying on merge protection.

## Documentation

- [Configuration](docs/configuration.md): attribution, protected code, monorepos, and rule fields.
- [Starter policy](examples/twokey.toml): opt-in examples of every rule type.
- [GitHub Actions](docs/github-actions.md): trusted workflows and required checks.
- [Security model](docs/security.md): guarantees and limitations.
- [Release process](docs/releasing.md): builds, distribution, and publishing.

## Development

Rust 2024, minimum Rust 1.90. CI tests the minimum version and stable on Linux,
macOS, and Windows. Keep `Cargo.lock` committed.

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo +1.90.0 test --locked
```

Formatting uses the Rust 2024 style edition, LF line endings, and a 120-column
limit. Cargo defines the shared Clippy policy: default lints plus checks for
debug macros, unfinished code, and stale lint suppressions (`#[expect]` instead
of `#[allow]`). Unsafe code is forbidden. Clippy reads the MSRV from
`rust-version`; CI runs it on Rust 1.90 and stable across all three platforms.

Run shell integration tests on Linux or macOS (macOS requires GNU sed):

```sh
cargo build --locked
bash tests/e2e.sh target/debug/twokey
```

## Contributing

Open an [issue](https://github.com/Azazel-Labs/twokey/issues) for bug reports or
feature requests, or submit a [pull request](https://github.com/Azazel-Labs/twokey/pulls).
Include your OS, `twokey --version`, a minimal policy, the command, and expected
and actual results in bug reports. Remove credentials and private code from
reproductions. Keep pull requests focused, explain the behavior change, and run
the relevant checks above. Nick Darnell is the sole maintainer.

### Translations

Translated introductions live in `docs/i18n/README.<language>.md` and link to the
English reference documentation. English is canonical. Translation fixes and
additional languages are welcome: preserve commands, TOML keys, paths, and
relative links, and update the language navigation in every README. Use language
names rather than country flags.

## License

twokey is dedicated to the public domain under
[CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/).
See [LICENSE](LICENSE) for the full legal text. Third-party dependencies retain
their own licenses.
