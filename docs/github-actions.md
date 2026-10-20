# GitHub Actions setup

[Back to README](../README.md)

The integration blocks AI-attributed changes to protected code. GitHub handles
human reviews; approving a PR does not override a twokey denial.

## Setup

1. Commit `.twokey.toml` with the paths, symbols, queries, and markers AI must
   not change. Run `twokey init` and uncomment the rules you want in the
   [starter policy](../examples/twokey.toml).
2. Copy [run.sh](../integrations/github/run.sh) to `.github/twokey/run.sh` and
   [twokey.yml](../integrations/github/twokey.yml) to `.github/workflows/twokey.yml`.
   Set the action commit SHA, CLI version, and Linux x86_64 musl archive checksum
   from the chosen release's `SHA256SUMS`. Use v0.2.0 or newer for this policy
   model; releases through v0.1.2 contain the old approval model.
3. Create a GitHub environment named `twokey`, limited to protected base branches.
   The workflow uses `GITHUB_TOKEN` by default. Keep `TWOKEY_DRY_RUN` unset or
   `false` for enforcement.
4. Merge the configuration into the default branch and every protected PR base.
   Open a test PR to generate the status. In **Settings → Rules → Rulesets**,
   create an **Active** branch ruleset for `main`. Require PRs and the
   **`twokey/policy`** status, restrict bypass permissions, and block force pushes.
5. Protect policy and workflow files using GitHub's CODEOWNERS and review rules.
   For other protected bases, set `TWOKEY_BASES` (for example `main,release`)
   and require **`twokey/policy (<branch>)`** for each non-default base.

Checks run when a PR opens, receives new commits (`synchronize`), reopens,
becomes ready for review, or changes base. The trusted `pull_request_target`
workflow reads the target branch's policy and fetches PR commits as data. It
never checks out or executes PR code. Keep it that way when customizing it.
The runner posts a status for the checked head and updates a summary comment.
It does not fetch reviews, maintain reviewer groups, or resolve team membership.

Actions runs after GitHub receives a push. Required checks prevent merging;
they cannot reject the initial upload of a feature branch. See the README's
[local checks](../README.md#can-twokey-stop-a-commit-or-push) for earlier feedback.

## Optional dedicated status identity

A dedicated GitHub App lets branch protection accept the status only from that
App. This prevents another workflow using `GITHUB_TOKEN` from impersonating the
status publisher. It is an authentication option, not an approval system.

Create and install an App with **Commit statuses: write**. In the `twokey`
environment, set `TWOKEY_APP_CLIENT_ID` to its Client ID and `TWOKEY_APP_KEY` to
its private key. After a check has run, select the App as the required status's
expected source. Keep the private key restricted to trusted workflows and
protected environments. Without an App, GitHub's organization rulesets can
require a trusted workflow where that feature is available.

## Migrating older integrations

Replace the runner and workflow together, remove `twokey-review-trigger.yml`,
and update policies using the [migration notes](configuration.md#migrating-from-the-approval-model).
No review-triggered rechecks or organization membership permissions are needed.
If using the optional App, use `TWOKEY_APP_CLIENT_ID` rather than the old
`TWOKEY_APP_ID` setting.
