# twokey

**Human approval for the code that matters.**

[![CI](https://github.com/Azazel-Labs/twokey/actions/workflows/ci.yml/badge.svg)](https://github.com/Azazel-Labs/twokey/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Azazel-Labs/twokey)](https://github.com/Azazel-Labs/twokey/releases)
[![crates.io](https://img.shields.io/crates/v/twokey.svg)](https://crates.io/crates/twokey)
[![License: CC0-1.0](https://img.shields.io/badge/license-CC0--1.0-blue.svg)](LICENSE)
[![Rust: 1.90+](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](Cargo.toml)
[![Edition: 2024](https://img.shields.io/badge/edition-2024-orange.svg)](Cargo.toml)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)](#installation)

English · [Español](docs/i18n/README.es.md) · [Français](docs/i18n/README.fr.md) · [Deutsch](docs/i18n/README.de.md) · [日本語](docs/i18n/README.ja.md) · [简体中文](docs/i18n/README.zh-CN.md)

[Quick start](#quick-start) · [Configuration](docs/configuration.md) · [GitHub Actions](docs/github-actions.md) · [Security model](docs/security.md) · [Contributing](#contributing)

twokey checks code changes against your repository's policy. Block AI-authored
edits to protected files or require approval from specific people before a
change can merge. The name comes from the two-person rule: sensitive changes
need a second, human key.

Checks are deterministic: file patterns, syntax trees, source markers, regular
expressions, and change-size limits. No model or external AI service is involved.

## Features

- **Protect sensitive code:** match files, functions, classes, or syntax patterns.
- **Require the right reviewer:** name individuals, policy groups, or GitHub teams.
- **Protect the policy itself:** CI reads policy from the base revision.
- **Get feedback early:** use a Git pre-commit hook or the Claude Code integration.
- **Enforce writing conventions:** [ban em dashes or unwanted wording](docs/configuration.md#ban-characters-or-wording) with content rules.
- **Enforce at merge time:** combine trusted CI with a required status check.

Syntax-aware rules support **Rust, Python, JavaScript, TypeScript/TSX, Go, Java,
C#, and Ruby**. Path and content rules work with other file types too.

## Installation

Use the **GitHub Action for CI** or **Cargo for local development**.

### GitHub Action (recommended for CI)

Add these steps to your workflow to install a prebuilt binary without a Rust
toolchain or compilation:

```yaml
- name: Install twokey
  uses: Azazel-Labs/TwoKey@6ebf78b9bdcfa412df2b505fea00a6f4bd495a77 # v0.1.1
  with:
    version: v0.1.1

- run: twokey --version
```

The [Install twokey action](action.yml) verifies the release checksum and adds
`twokey` to `PATH` for subsequent steps. It supports the Linux, macOS, and Windows
platforms listed below. The `uses` SHA pins the installer code; `version` selects
the CLI release. For a pinned archive checksum too, set the `sha256` input to the
matching platform's entry in the release's
[SHA256SUMS](https://github.com/Azazel-Labs/TwoKey/releases/download/v0.1.1/SHA256SUMS).

To **enforce policy on pull requests**, follow the [GitHub Actions setup guide](docs/github-actions.md).
It supplies the trusted workflow and runner that read policy from the base
revision, collect approvals for the exact head commit, and publish the
`twokey/policy` status to require in branch protection. The install step alone
does not run policy checks. The integration can use `GITHUB_TOKEN`; a dedicated
GitHub App is optional and lets branch protection bind results to that App.

### Cargo (recommended for local use)

Install the published CLI from [crates.io](https://crates.io/crates/twokey):

```sh
cargo install twokey --locked
twokey --version
```

To select a specific release, use `cargo install twokey --version 0.1.1 --locked`.
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
(ARM64 requires Windows x64 emulation). Use `-Version v0.1.1`, `-To <directory>`,
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

## Quick start

Create `.twokey.toml` in the repository you want to protect:

```toml
version = 1

[approvals]
approvers = ["your-github-login"]

[[rule]]
id = "protect-ci"
kind = "path"
paths = ["/.github/workflows/**", "/.github/CODEOWNERS", "/CODEOWNERS"]
action = "deny"
reason = "CI and review routing must be changed by a human."

[[rule]]
id = "review-auth"
kind = "path"
paths = ["/src/auth/**"]
action = "approve"
reason = "Authentication changes require human review."
```

This is the [minimal example](examples/minimal.toml). Replace the approver with
an actual GitHub login and adapt the paths. These rules apply to AI-attributed
changes; ordinary human changes are allowed by default. twokey's own policy
and integration wiring also have built-in protection.

Validate and preview the local policy:

```sh
twokey validate --policy .twokey.toml
twokey scan --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

`--ai` treats the preview as AI-authored. `--dry-run` reports without blocking.
`scan` examines committed files at `HEAD`, so the repository needs a commit.
Commit the policy before enabling enforcement. For a larger starting point,
`twokey init` writes the [fully commented example](examples/twokey.toml).

## Usage

```sh
# Check staged changes using the committed policy.
twokey check --staged

# Evaluate a pull request using policy from its target branch.
twokey check --base origin/main --head HEAD --require-policy

# Produce structured output.
twokey check --staged --format json
```

Replace `origin/main` with your target branch. In CI, the supplied runner obtains
approvals for the exact head commit from GitHub. `--approver` is a trusted input,
not proof of review on its own.

| Exit code | Meaning |
| --- | --- |
| `0` | Passed, or report-only mode |
| `1` | Blocked by policy |
| `2` | Error, including invalid policy |
| `3` | Human approval required |

### Integrations

| Integration | Setup | Purpose |
| --- | --- | --- |
| GitHub Actions | [Setup guide](docs/github-actions.md) | Enforce policy with a required status check |
| Git pre-commit | Copy [pre-commit](integrations/git/pre-commit) into `.githooks/`, then run `git config core.hooksPath .githooks` | Check staged edits locally |
| Claude Code | Merge [hook settings](integrations/claude/settings.json) into `.claude/settings.json` | Check edits and report protected changes after shell commands |

Local hooks can be bypassed. Trusted CI, branch protection, and restricted agent
credentials provide enforcement. Metadata-based AI attribution is a heuristic;
use stricter attribution settings when needed. Read the
[security model](docs/security.md) before relying on merge protection.

## Documentation

- [Configuration](docs/configuration.md): attribution, approvers, monorepos, and rule fields.
- [Minimal policy](examples/minimal.toml): a small starting point.
- [Complete policy](examples/twokey.toml): commented examples of every rule type.
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
