# GitHub Actions setup

[Back to README](../README.md)

## GitHub setup

1. Copy `integrations/github/run.sh` to `.github/twokey/run.sh`, and
   `twokey.yml` + `twokey-review-trigger.yml` to `.github/workflows/`. Fill in
   the release URL and sha256 in `twokey.yml`.
2. **Create a GitHub App** with *Commit statuses: write*, *Pull requests: read*
   and, for team approvers, organization *Members: read*. Install it on the repo.
3. Create an environment named `twokey`, limited to your protected branches,
   holding variable `TWOKEY_APP_CLIENT_ID` (the App's Client ID from its settings)
   and secret `TWOKEY_APP_KEY`.
4. Branch ruleset on `main`: require status check **`twokey/policy`** with the
   expected source set to **your App**; require PRs, dismiss stale approvals on
   push, CODEOWNERS on `.github/`, `.twokey.toml` and sub-policies.
5. Other protected bases (e.g. `release`): list them in `TWOKEY_BASES` on the
   twokey step and require **`twokey/policy (release)`** on that branch. The
   check only vouches for listed bases, and each base gets its own status name,
   because commit statuses belong to a commit: otherwise a second PR with the
   same head commit against a weaker branch could satisfy the check on `main`.
6. Keep the third-party actions pinned to commit SHAs when updating your copies.

When upgrading an older copy of the template, replace `TWOKEY_APP_ID` with
`TWOKEY_APP_CLIENT_ID` in the `twokey` environment and use the App's Client ID
from its settings. The current token action deprecates the old `app-id` input.

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
