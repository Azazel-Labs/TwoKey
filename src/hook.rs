//! Agent hook mode (Claude Code `PreToolUse` / `PostToolUse`).
//!
//! PreToolUse: simulates the pending edit in memory, runs it through the same
//! rules as CI and denies protected edits, so the agent learns about a
//! protected area before it spends a whole session on it.
//!
//! PostToolUse on Bash: shell commands can write files directly (`sed -i`, `>`,
//! code generators), which no PreToolUse check can predict. So before each Bash
//! command we snapshot the working tree's findings, and afterwards we report
//! only the *new* ones, telling the agent to revert anything it may not touch.
//!
//! This is a seatbelt only: CI on the pull request remains the authority.

use crate::attribution::{Force, for_local};
use crate::config::{self, PolicyArgs};
use crate::engine::{Evaluation, Verdict, evaluate};
use crate::git::{FileChange, Repo, Status};
use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize)]
struct HookInput {
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    hook_event_name: String,
    #[serde(default)]
    tool_use_id: Option<String>,
}

enum Decision {
    Pass,
    Deny(String),
    /// PostToolUse: feed back to the agent as a blocking error.
    PostBlock(String),
    /// PostToolUse: non-blocking context for the agent.
    PostContext(String),
}

/// `event` is the hook event being answered (PostToolUse or PostToolUseFailure
/// for the post-* decisions).
fn emit(d: Decision, event: &str) {
    let out = match d {
        Decision::Pass => return,
        Decision::Deny(r) => pre("deny", r),
        Decision::PostBlock(r) => json!({ "decision": "block", "reason": r }),
        Decision::PostContext(c) => json!({
            "hookSpecificOutput": { "hookEventName": event, "additionalContext": c }
        }),
    };
    println!("{out}");
}

fn pre(decision: &str, reason: String) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision,
            "permissionDecisionReason": reason,
        }
    })
}

pub fn run_claude(repo_hint: &Path, policy_args: &PolicyArgs) -> Result<i32> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("reading hook input")?;
    let event = serde_json::from_str::<Value>(&buf)
        .ok()
        .and_then(|v| v.get("hook_event_name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "PreToolUse".into());
    let is_post = is_post_event(&event);
    let decision = match decide(&buf, repo_hint, policy_args) {
        Ok(d) => d,
        // After the fact we can only inform; before, fail closed.
        Err(e) if is_post => Decision::PostContext(format!("twokey could not check this command's effects ({e:#}).")),
        Err(e) => Decision::Deny(format!(
            "twokey could not evaluate this action ({e:#}); the check must succeed before this action can proceed."
        )),
    };
    emit(decision, &event);
    Ok(0)
}

/// PostToolUse fires for successful tool calls, PostToolUseFailure for failed
/// ones; a failed Bash command can still have written files.
fn is_post_event(e: &str) -> bool {
    e == "PostToolUse" || e == "PostToolUseFailure"
}

fn decide(input: &str, repo_hint: &Path, policy_args: &PolicyArgs) -> Result<Decision> {
    let hi: HookInput = serde_json::from_str(input).context("hook input is not valid JSON")?;
    let cwd = hi
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_hint.to_path_buf());
    let is_edit = matches!(hi.tool_name.as_str(), "Write" | "Edit" | "MultiEdit" | "NotebookEdit");
    let target = hi
        .tool_input
        .get("file_path")
        .or_else(|| hi.tool_input.get("notebook_path"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|f| soft_canonicalize(&abs_path(&cwd, f)));

    if is_edit && let Some(t) = &target {
        // Writing into .git (hooks, config: core.hooksPath, core.fsmonitor, ...)
        // is code execution and guardrail tampering, in any repository.
        if t.components()
            .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"))
        {
            return Ok(Decision::Deny(format!(
                "twokey: {} is inside a .git directory; agents may not edit git internals (hooks, config).",
                t.display()
            )));
        }
    }
    // Find the repository from the file being edited (not the session's cwd), so
    // nested repos are judged by their own policy.
    let discover_from = match (&target, is_edit) {
        (Some(t), true) => nearest_existing_dir(t),
        _ => cwd.clone(),
    };
    let repo = match Repo::discover(&discover_from) {
        Ok(r) => r,
        Err(_) => {
            // Outside any repository. Agent configuration is still a guardrail.
            if let Some(t) = target.filter(|_| is_edit)
                && t.components().any(|c| c.as_os_str() == ".claude")
            {
                // User-level agent settings that register twokey: never by AI.
                let mentions = |s: &str| s.to_ascii_lowercase().contains("twokey");
                let before = std::fs::read_to_string(&t).map(|s| mentions(&s)).unwrap_or(false);
                let after = hi
                    .tool_input
                    .get("content")
                    .or_else(|| hi.tool_input.get("new_string"))
                    .and_then(Value::as_str)
                    .map(mentions)
                    .unwrap_or(false);
                if before || after {
                    return Ok(Decision::Deny(format!(
                        "twokey: {} registers twokey as an agent hook; agents may not change it. Ask the user to make this change.",
                        t.display()
                    )));
                }
                return Ok(Decision::Deny(format!(
                    "twokey: {} is agent configuration outside any repository; a human must make this edit.",
                    t.display()
                )));
            }
            return Ok(Decision::Pass);
        }
    };
    let head = repo.has_head().then_some("HEAD");
    let mut policy = config::load(&repo, policy_args, head)?;
    // Prefer the policy on the upstream base: the agent can commit to HEAD, so a
    // HEAD policy is one it could have weakened itself.
    if policy_args.policy.is_none()
        && policy_args.policy_ref.is_none()
        && let Some(base) = policy.settings.default_base.clone()
        && let Some(sha) = repo.try_rev_parse(&base)
    {
        policy = config::load(&repo, policy_args, Some(&sha))?;
    }
    let att = for_local(&policy.ai, repo.current_branch().as_deref(), Force::Ai, true, None);

    if hi.tool_name == "Bash" {
        let snap = snapshot_path(&repo, hi.tool_use_id.as_deref());
        if is_post_event(&hi.hook_event_name) {
            let background = hi
                .tool_input
                .get("run_in_background")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            return post_bash(&repo, &policy, &att, snap, background);
        }
        // PreToolUse: remember what was already flagged before this command runs.
        // Best effort: a snapshot failure must never block an ordinary command.
        if let (Some(p), Ok(changes)) = (&snap, worktree_changes(&repo)) {
            let before = evaluate(&policy, &changes, &att);
            let keys = finding_keys(&before, &changes);
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(bytes) = serde_json::to_vec(&keys) {
                let _ = std::fs::write(p, bytes);
            }
        }
    }

    if is_post_event(&hi.hook_event_name) {
        return Ok(Decision::Pass); // only Bash needs an after-the-fact check
    }

    let changes = match hi.tool_name.as_str() {
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            match simulate_edit(&repo, &cwd, &hi.tool_name, &hi.tool_input)? {
                Some(c) => c,
                None => return Ok(Decision::Pass),
            }
        }
        "Bash" => {
            let cmd = hi.tool_input.get("command").and_then(Value::as_str).unwrap_or("");
            match git_intent(cmd) {
                Some(GitIntent::Commit) => worktree_changes(&repo)?,
                Some(GitIntent::Push) => {
                    let Some(base) = policy.settings.default_base.clone() else {
                        return Ok(Decision::Pass);
                    };
                    let Some(base_sha) = repo.try_rev_parse(&base) else {
                        return Ok(Decision::Pass);
                    };
                    let mb = repo.merge_base(&base_sha, "HEAD")?;
                    let raw = repo.diff_raw(&[mb.as_str(), "HEAD"])?;
                    repo.load_changes(&raw, false)?
                }
                None => return Ok(Decision::Pass),
            }
        }
        _ => return Ok(Decision::Pass),
    };
    if changes.is_empty() {
        return Ok(Decision::Pass);
    }
    let eval = evaluate(&policy, &changes, &att);
    Ok(to_decision(&eval))
}

/// Identity of a blocking finding *and the file content it was found in*, so a
/// file that was already dirty before a command but changed again by it is
/// reported again.
fn finding_key(f: &crate::engine::Finding, content: &std::collections::HashMap<&str, u64>) -> String {
    format!(
        "{}|{}|{}|{}|{}|{:x}",
        f.rule,
        f.path,
        f.symbol.as_deref().unwrap_or(""),
        f.change,
        f.message,
        content.get(f.path.as_str()).copied().unwrap_or(0)
    )
}

fn finding_keys(e: &Evaluation, changes: &[FileChange]) -> Vec<String> {
    use std::hash::{Hash, Hasher};
    let content: std::collections::HashMap<&str, u64> = changes
        .iter()
        .map(|c| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (&c.new, c.new_mode).hash(&mut h);
            (c.path.as_str(), h.finish())
        })
        .collect();
    e.findings
        .iter()
        .filter(|f| f.blocking())
        .map(|f| finding_key(f, &content))
        .collect()
}

/// `<git dir>/twokey/bash-<id>.json`, or None without a usable tool_use_id.
fn snapshot_path(repo: &Repo, id: Option<&str>) -> Option<PathBuf> {
    let id: String = id?
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(100)
        .collect();
    if id.is_empty() {
        return None;
    }
    Some(repo.git_dir().ok()?.join("twokey").join(format!("bash-{id}.json")))
}

fn post_bash(
    repo: &Repo,
    policy: &crate::config::Policy,
    att: &crate::attribution::Attribution,
    snap: Option<PathBuf>,
    background: bool,
) -> Result<Decision> {
    let before: Vec<String> = snap
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if let Some(p) = &snap {
        let _ = std::fs::remove_file(p);
    }
    if background {
        // The hook fires when a background command starts, not when it ends.
        return Ok(Decision::PostContext(
            "twokey can't check files a background command writes; they will be checked at commit and in CI.".into(),
        ));
    }
    let changes = worktree_changes(repo)?;
    let after = evaluate(policy, &changes, att);
    if !after.enforced {
        return Ok(Decision::Pass);
    }
    let after_keys = finding_keys(&after, &changes);
    let new: Vec<&crate::engine::Finding> = after
        .findings
        .iter()
        .filter(|f| f.blocking())
        .zip(after_keys.iter())
        .filter(|(_, k)| !before.contains(k))
        .map(|(f, _)| f)
        .collect();
    if new.is_empty() {
        return Ok(Decision::Pass);
    }
    let line = |f: &&crate::engine::Finding| format!("- [{}] {} — {}", f.rule, f.path, f.message);
    let denied: Vec<String> = new
        .iter()
        .filter(|f| f.action == crate::config::Action::Deny)
        .map(line)
        .collect();
    if !denied.is_empty() {
        // Careful wording: a person may have edited these files while the command
        // ran, and the agent must not destroy their work.
        return Ok(Decision::PostBlock(format!(
            "twokey: files AI agents may not modify changed while that command ran:\n{}\nIf your command made these changes, undo exactly those changes (restore the previous content). If you did not make them, leave the files alone. Do not delete files or discard other uncommitted work, do not try another route, and tell the user what change is needed so a human can make it.",
            denied.join("\n")
        )));
    }
    Ok(Decision::Pass)
}

fn to_decision(e: &Evaluation) -> Decision {
    if !e.enforced {
        return Decision::Pass; // report-only rollout: never interrupt the agent
    }
    let lines: Vec<String> = e
        .findings
        .iter()
        .filter(|f| f.blocking())
        .map(|f| {
            let loc = match f.line {
                Some(l) => format!("{}:{l}", f.path),
                None => f.path.clone(),
            };
            let mut s = format!("- [{}] {} — {}", f.rule, loc, f.message);
            if let Some(r) = &f.reason {
                s.push_str(&format!(" ({r})"));
            }
            s
        })
        .collect();
    match e.verdict {
        Verdict::Pass => Decision::Pass,
        Verdict::Blocked => Decision::Deny(format!(
            "twokey: this change touches areas AI agents may not modify:\n{}\nDo not try to work around this (other tools, renames, symlinks, generated files). Stop and tell the user what change is needed so a human can make it.",
            lines.join("\n")
        )),
    }
}

fn abs_path(cwd: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
}

fn nearest_existing_dir(p: &Path) -> PathBuf {
    let mut cur = p.to_path_buf();
    loop {
        if cur.is_dir() {
            return cur;
        }
        if !cur.pop() {
            return PathBuf::from(".");
        }
    }
}

/// Resolve the tool's target path to a repo-relative path, following symlinks so
/// writing through a link into a protected file is evaluated as the real file.
fn rel_paths(repo: &Repo, cwd: &Path, file: &str) -> Vec<String> {
    let p = Path::new(file);
    let abs = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    let mut out = Vec::new();
    for cand in [lexical_normalize(&abs), soft_canonicalize(&abs)] {
        if let Ok(rel) = cand.strip_prefix(&repo.root) {
            let s = rel.to_string_lossy().replace('\\', "/");
            if !s.is_empty() && !s.starts_with(".git/") && !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalize the longest existing ancestor, then append the rest.
fn soft_canonicalize(p: &Path) -> PathBuf {
    let p = lexical_normalize(p);
    let mut existing = p.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(c) = existing.canonicalize() {
            let mut out = c;
            for r in rest.iter().rev() {
                out.push(r);
            }
            return out;
        }
        match (existing.file_name().map(|s| s.to_os_string()), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => return p,
        }
    }
}

fn simulate_edit(repo: &Repo, cwd: &Path, tool: &str, input: &Value) -> Result<Option<Vec<FileChange>>> {
    let file = input
        .get("file_path")
        .or_else(|| input.get("notebook_path"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if file.is_empty() {
        return Ok(None);
    }
    let rels = rel_paths(repo, cwd, file);
    if rels.is_empty() {
        return Ok(None); // outside the repository
    }
    let abs = {
        let p = Path::new(file);
        if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
    };
    let old = std::fs::read(&abs).ok();
    let old_text = old.as_ref().map(|b| String::from_utf8_lossy(b).into_owned());
    let new: Vec<u8> = match tool {
        "Write" => input
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .as_bytes()
            .to_vec(),
        "Edit" => {
            let Some(cur) = &old_text else { return Ok(None) };
            match apply_edit(cur, input) {
                Some(s) => s.into_bytes(),
                None => return Ok(None), // the tool itself will reject this edit
            }
        }
        "MultiEdit" => {
            let Some(cur) = &old_text else { return Ok(None) };
            let mut s = cur.clone();
            for e in input
                .get("edits")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                match apply_edit(&s, &e) {
                    Some(n) => s = n,
                    None => return Ok(None),
                }
            }
            s.into_bytes()
        }
        // Notebook cell edits: we can't cheaply reconstruct the result, so only
        // path-level rules see this (content is marked as changed).
        _ => {
            let mut b = old.clone().unwrap_or_default();
            b.extend_from_slice(b"\n");
            b
        }
    };
    let status = if old.is_some() { Status::Modified } else { Status::Added };
    Ok(Some(
        rels.into_iter()
            .map(|path| FileChange {
                path,
                status,
                old_mode: if old.is_some() { 0o100644 } else { 0 },
                new_mode: 0o100644,
                old: old.clone(),
                new: Some(new.clone()),
            })
            .collect(),
    ))
}

fn apply_edit(cur: &str, e: &Value) -> Option<String> {
    let old = e.get("old_string").and_then(Value::as_str)?;
    let new = e.get("new_string").and_then(Value::as_str).unwrap_or("");
    let all = e.get("replace_all").and_then(Value::as_bool).unwrap_or(false);
    if old.is_empty() {
        return Some(format!("{new}{cur}"));
    }
    if !cur.contains(old) {
        return None;
    }
    Some(if all {
        cur.replace(old, new)
    } else {
        cur.replacen(old, new, 1)
    })
}

#[derive(Debug, PartialEq, Eq)]
enum GitIntent {
    Commit,
    Push,
}

fn git_intent(cmd: &str) -> Option<GitIntent> {
    // `git [global opts] commit|push`, anywhere in a compound command.
    // Matches inside `sh -c '...'`, backticks, $(...), and /usr/bin/git too.
    let re = Regex::new(
        r#"(?:^|[^\w.-])(?:\S*/)?git(?:\s+(?:-[Cc]\s+(?:"[^"]*"|'[^']*'|\S+)|--?[\w-]+(?:=(?:"[^"]*"|'[^']*'|\S+))?))*\s+(commit|push)\b"#,
    )
    .ok()?;
    let mut intent = None;
    for c in re.captures_iter(cmd) {
        match &c[1] {
            "commit" => return Some(GitIntent::Commit),
            _ => intent = Some(GitIntent::Push),
        }
    }
    intent
}

/// Everything that a `git commit` could include: the index (what a plain commit
/// takes), tracked working-tree changes (`-a`, or `git add` in the same command)
/// and untracked files. Staged content that differs from the working tree is
/// kept as its own entry, so "stage it, then restore the file" doesn't hide it.
pub fn worktree_changes(repo: &Repo) -> Result<Vec<FileChange>> {
    let base = if repo.has_head() {
        "HEAD"
    } else {
        crate::git::EMPTY_TREE
    };
    let raw = repo.diff_raw(&[base])?;
    let mut v = repo.load_changes(&raw, true)?;
    let staged_raw = repo.diff_raw(&["--cached", base])?;
    for s in repo.load_changes(&staged_raw, false)? {
        let dup = v
            .iter()
            .any(|w| w.path == s.path && w.new == s.new && w.new_mode == s.new_mode);
        if !dup {
            v.push(s);
        }
    }
    v.extend(repo.untracked_changes()?);
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_git_commands() {
        assert_eq!(git_intent("git commit -m x"), Some(GitIntent::Commit));
        assert_eq!(
            git_intent("cd a && git add -A && git commit -am 'x'"),
            Some(GitIntent::Commit)
        );
        assert_eq!(git_intent("git -C repo -c user.name=x commit"), Some(GitIntent::Commit));
        assert_eq!(git_intent("git push origin HEAD"), Some(GitIntent::Push));
        assert_eq!(git_intent("echo git commit"), Some(GitIntent::Commit)); // conservative
        assert_eq!(git_intent("sh -c 'git commit -m x'"), Some(GitIntent::Commit));
        assert_eq!(git_intent("/usr/bin/git commit"), Some(GitIntent::Commit));
        assert_eq!(git_intent("echo `git commit`"), Some(GitIntent::Commit));
        assert_eq!(git_intent(r#"git -c "user.name=A B" commit"#), Some(GitIntent::Commit));
        assert_eq!(git_intent("git status"), None);
        assert_eq!(git_intent("legit commit"), None);
    }

    #[test]
    fn edit_simulation() {
        let v = json!({"old_string": "a", "new_string": "b"});
        assert_eq!(apply_edit("a a", &v).as_deref(), Some("b a"));
        let v = json!({"old_string": "a", "new_string": "b", "replace_all": true});
        assert_eq!(apply_edit("a a", &v).as_deref(), Some("b b"));
        assert_eq!(apply_edit("x", &v), None);
    }
}
