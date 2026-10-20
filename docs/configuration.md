# Configuration

[Back to README](../README.md)

## Deciding what's "AI"

`[ai].assume_ai` defaults to `true`: untagged changes are treated as AI-assisted.
Choose the literal human declaration with `human_marker`:

```toml
[ai]
assume_ai = true
human_marker = "#Human" # default: "Human-Authored: true"
```

This is a literal whole-line match, not a regex. It is case-insensitive and must
appear exactly once in the final paragraph after a blank line. Empty markers,
control characters, and surrounding whitespace in the configuration are errors.
Changing the marker replaces the default; the old declaration no longer counts.

With the default marker, a fully human commit looks like:

```text
Fix token validation

Human-Authored: true
```

With `human_marker = "#Human"`, use `git commit -m "Fix token validation" -m "#Human"`.
For editor-based commits, run `git config core.commentChar ';'` so Git does not
strip the `#Human` line as a comment. CI reads the final stored commit message.
Other lines in the final paragraph must be Git-style `Key: value` trailers.

A prose mention, a subject-only flag, duplicate declarations, or a different
marker value does not mark a commit human.
Explicit AI signals take precedence over the declaration:

- AI message patterns (`Co-authored-by: Claude …`, `Assisted-by:`, `Generated-by:`)
- author/committer identities (`noreply@anthropic.com`, `[bot]`)
- branch names (`claude/…`, `codex/…`, `agent/…`), which mark the whole range AI
- environment variables for local checks (`CLAUDECODE`, `TWOKEY_AI`)

Any AI commit touching a file in the range makes that file AI-attributed. A later
human declaration does not undo that attribution. Mixed/AI-assisted commits must
not use the human declaration. Untagged merge commits are also AI-attributed.

Staged/worktree checks assume AI because no commit message exists yet. Use the
bundled `commit-msg` hook, which passes `--commit-message-file FILE`, to check a
proposed declaration against staged changes. This option cannot be used with
`--base`, `--worktree`, `--ai`, or `--human`. Agent hooks always treat edits as AI.
`--human` remains an explicit override for trusted manual checks, never the CI
runner. It does not add a declaration to the eventual commit.

A declaration is self-reported, not proof of authorship. TwoKey does not verify
commit signatures or manage signing keys.

Set `assume_ai = false` for signal-only attribution. This requires an explicit,
non-empty `trailers` list in `[ai]`; missing or empty lists are policy errors
(exit 2). Blank patterns are also rejected. For example:

```toml
[ai]
assume_ai = false
trailers = ['(?i)^co-authored-by:.*\b(claude|copilot|codex)\b', '(?i)^(ai-)?(assisted|generated)-by:']
```

The starter policy includes active trailer patterns directly below `assume_ai`.
With `assume_ai = true`, omitted trailers still use the built-in defaults.

## Enforcement

Rules default to `action = "deny"` for AI-attributed changes. `warn` reports a
finding without blocking; `allow` adds no finding. Rules are additive: an allow
rule does not override a denial from another rule. Human changes default to
`human_action = "allow"`; set it to `deny` for conventions that apply to everyone.

There are no reviewers, groups, approval tokens, or approval overrides. GitHub
manages reviews independently; a reviewed AI change still cannot touch protected
code. Attribution is based on configured signals, not a model that estimates
what percentage of a change was generated. In mixed changes, a file touched by
any AI-attributed commit is treated as AI-attributed.

### Migrating from the approval model

Untagged changes now default to AI. `assume_ai = true` accepts the explicit human
trailer when no AI signal conflicts; it no longer forces every commit to AI.
Use `--ai` in trusted checks that must treat every change as AI regardless of
trailers. Existing policies can opt back into signal-only attribution with
`[ai].assume_ai = false` and explicitly configuring a non-empty `trailers` list.

Remove `[approvals]`, all `approvers` fields, and `[markers].review_action`.
Replace `action = "approve"` with `action = "deny"` for code AI must not touch.
Remove `human_action = "approve"` to leave human review enforcement to GitHub.
Old fields and action values are rejected; they are not silently ignored.
Remove `--approver`, `--teams-file`, and calls to `twokey teams` from integrations.
Exit code 3 is no longer used. Legacy `twokey:review-*` markers are treated as
locks; rename them to `twokey:lock-*` when migrating. Use the updated GitHub
runner and remove `twokey-review-trigger.yml` from consuming repositories.

## Monorepos

`settings.include = ["services/*/.twokey.toml"]` pulls in per-directory
policies. A sub-policy may only contain `[[rule]]` tables; its paths are relative
to its directory (no `paths` = the whole directory), its rule ids are prefixed
with the directory, and it can't change attribution, limits or
anything else in the root policy. Rules only add findings, so a team can tighten
its own directory but never loosen anyone else's. Sub-policy files are protected
like the root policy.

## Policy reference

### Ban characters or wording

Content rules can enforce writing conventions as well as code restrictions.
For example, copy this rule into `.twokey.toml` to ban em dashes:

```toml
[[rule]]
id = "no-em-dashes"
kind = "content"
pattern = '\x{2014}'
lines = "added"
action = "deny"
human_action = "deny"
reason = "Use a comma, colon, parentheses, or a separate sentence instead of an em dash."
```

The single quotes preserve the regex escape for Unicode U+2014. This checks
added lines, including replacement lines in an edit, in all text files. Removing
an em dash is allowed; an existing em dash on an untouched line is not flagged.
An edited line that still contains an em dash is blocked. Ordinary hyphens and
en dashes are allowed.

Omit `human_action` to apply the ban only to AI-attributed changes. Add
`paths = ["*.md", "*.txt", "*.rst"]` to limit it to prose. To ban en dashes too,
use `pattern = '[\x{2013}\x{2014}]'`. Other content regexes can ban unwanted
phrases, such as `pattern = '(?i)\b(obviously|simply)\b'`.

This rule is commented out in the [starter policy](../examples/twokey.toml)
generated by `twokey init`; uncomment its block to enable it. Checks use the committed policy, so commit policy
changes before checking staged edits.

### Rule fields

See [complete example](../examples/twokey.toml) for a commented, complete policy. Rule fields:

```toml
[[rule]]
id = "unique-id"
kind = "path" | "symbol" | "query" | "content"
action = "deny" | "warn" | "allow"   # for AI changes (default deny)
human_action = "allow"                           # for human changes (default allow)
paths = ["/src/auth/**", "*.pem"]                # leading / anchors; no slash = any depth
exclude = ["**/fixtures/**"]
on = ["added", "modified", "deleted"]            # default: all
reason = "shown to reviewers and to the agent"
# symbol:  symbols = ["verify_*", "Session::*"]   language = "rust" (optional)
# query:   language = "rust"  query = "(unsafe_block) @protect"  capture = "protect"
# content: pattern = '#\[ignore\]'  lines = "added" | "removed" | "both"
```
