#!/usr/bin/env bash
# Trusted twokey runner for GitHub. Install as .github/twokey/run.sh (twokey
# protects that path from AI changes automatically).
#
# Runs from the BASE branch checkout (pull_request_target), never
# from the PR. It treats the PR purely as git data: it fetches the head commit,
# never checks it out, never runs its code.
#
#   run.sh <pr-number>
#
# Env:
#   GH_TOKEN      github.token: reads PR and git data; writes the comment
#   APP_TOKEN     optional GitHub App token: posts the commit status (so the
#                 required check can be pinned to the App). Without it the
#                 status is posted with GH_TOKEN.
#   TWOKEY        path to the twokey binary (default: twokey on PATH)
#   TWOKEY_BASES  comma-separated base branches this check may vouch for
#                 (default: the repository's default branch). See below.
#   DRY_RUN       "true" to report without failing (rollout)
set -Eeuo pipefail

pr="${1:?usage: run.sh <pr-number>}"
[[ "$pr" =~ ^[0-9]+$ ]] || { echo "invalid PR number: $pr" >&2; exit 2; }
repo="${GITHUB_REPOSITORY:?}"
twokey="${TWOKEY:-twokey}"
status_token="${APP_TOKEN:-$GH_TOKEN}"
run_url="${GITHUB_SERVER_URL:-https://github.com}/${repo}/actions/runs/${GITHUB_RUN_ID:-0}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# --- PR facts come from the API, not from the event payload -------------------
gh api "repos/$repo/pulls/$pr" > "$work/pr.json"
head_sha=$(jq -r .head.sha "$work/pr.json")
base_ref=$(jq -r .base.ref "$work/pr.json")
head_ref=$(jq -r .head.ref "$work/pr.json")
state=$(jq -r .state "$work/pr.json")
[[ "$head_sha" =~ ^[0-9a-f]{40}$ ]] || { echo "bad head sha" >&2; exit 2; }
if [[ "$state" != "open" ]]; then echo "PR #$pr is $state; nothing to do"; exit 0; fi

# --- Which bases may this check vouch for? -------------------------------------
# Commit statuses belong to a commit, not to a PR. Without this, someone could
# open a second PR with the same head commit against a branch with a weaker
# policy and have *its* "success" satisfy the check on main. So: only vouch
# for allowed bases, and give every non-default base its own status name.
default_branch=$(gh api "repos/$repo" --jq .default_branch)
allowed="${TWOKEY_BASES:-$default_branch}"
if [[ ",${allowed// /}," != *",${base_ref},"* ]]; then
  echo "base '$base_ref' is not in TWOKEY_BASES ($allowed); not posting a status"
  exit 0
fi
context="twokey/policy"
[[ "$base_ref" == "$default_branch" ]] || context="twokey/policy (${base_ref})"

status() { # status <state> <description>
  GH_TOKEN="$status_token" gh api -X POST "repos/$repo/statuses/$head_sha" \
    -f state="$1" -f context="$context" -f description="${2:0:139}" -f target_url="$run_url" >/dev/null
}
status pending "twokey is checking this change"
# Never leave the status stuck at "pending": any unexpected failure ends in error.
trap 'status error "twokey runner failed; see the run log" || true' ERR

# --- Git data: base branch tip + PR head commit (never checked out) -----------
auth="AUTHORIZATION: basic $(printf 'x-access-token:%s' "$GH_TOKEN" | base64 | tr -d '\n')"
gitf() { git -c "http.extraheader=$auth" fetch --no-tags --quiet origin "$@"; }
gitf "+refs/heads/${base_ref}:refs/remotes/origin/${base_ref}"
gitf "+refs/pull/${pr}/head:refs/twokey/pr-${pr}"
git cat-file -e "${head_sha}^{commit}"

# --- Evaluate ------------------------------------------------------------------
dry=()
[[ "${DRY_RUN:-}" == "true" ]] && dry=(--dry-run)
trap - ERR
set +e
"$twokey" check \
  --base "origin/${base_ref}" --head "$head_sha" \
  --branch "$head_ref" \
  --require-policy \
  --format github \
  --summary-out "$work/summary.md" \
  "${dry[@]}"
code=$?
set -e
trap 'status error "twokey runner failed; see the run log" || true' ERR

case $code in
  0) status success "No prohibited changes found" ;;
  1) status failure "Blocked: AI change to areas agents may not modify" ;;
  *) status error "twokey error (exit $code); see the run log" ;;
esac

# --- Sticky PR comment (updated in place, removed when there's nothing to say) --
# Best effort: the status above is what matters.
trap - ERR
set +e
marker='<!-- twokey-report -->'
existing=$(gh api --paginate "repos/$repo/issues/$pr/comments" \
  --jq ".[] | select(.user.login == \"github-actions[bot]\" and (.body | startswith(\"$marker\"))) | .id" 2>/dev/null \
  | sed -n 1p)
if [[ -s "$work/summary.md" ]] && grep -q '^| ' "$work/summary.md"; then
  body="$(head -c 60000 "$work/summary.md")"
  [[ $(wc -c < "$work/summary.md") -gt 60000 ]] && body+=$'\n\n…(truncated; see the run log)'
  body+=$'\n'"<sub>[run log]($run_url)</sub>"
  if [[ -n "$existing" ]]; then
    gh api -X PATCH "repos/$repo/issues/comments/$existing" -f body="$body" >/dev/null
  else
    gh api -X POST "repos/$repo/issues/$pr/comments" -f body="$body" >/dev/null
  fi
elif [[ -n "$existing" ]]; then
  gh api -X DELETE "repos/$repo/issues/comments/$existing" >/dev/null
fi
set -e

# Fail the job too, for visibility; the commit status is what branch rules require.
[[ $code == 0 ]] || exit 1
