//! Rule evaluation: file changes + policy + attribution -> verdict.

use crate::attribution::Attribution;
use crate::config::{Action, ChangeKind, LineSel, Policy, Rule, RuleSpec};
use crate::git::{FileChange, MODE_EXEC, MODE_SUBMODULE, MODE_SYMLINK, Status};
use crate::glob::normalize_rel;
use crate::lang::{Lang, parse};
use crate::markers;
use crate::symbols::{self, Capture, Symbol};
use serde::Serialize;
use similar::{ChangeTag, TextDiff};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;
use tree_sitter::Tree;

const MAX_HITS_PER_RULE_FILE: usize = 10;

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule: String,
    pub kind: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub change: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub ai: bool,
    pub action: Action,
}

impl Finding {
    pub fn blocking(&self) -> bool {
        self.action == Action::Deny
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Blocked,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Blocked => "BLOCKED",
        }
    }
    pub fn exit_code(self) -> i32 {
        match self {
            Verdict::Pass => 0,
            Verdict::Blocked => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Evaluation {
    pub verdict: Verdict,
    /// False in report-only mode (`--dry-run` / `settings.enforce = false`):
    /// the verdict is what *would* happen; the exit code is 0.
    pub enforced: bool,
    pub policy: String,
    pub files_changed: usize,
    pub lines_changed: usize,
    pub attribution: Attribution,
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
}

/// Lazily parsed view of one side of a file.
struct Side<'a> {
    bytes: Option<&'a [u8]>,
    text: Option<String>,
    tree: Option<Option<Tree>>,
}

impl<'a> Side<'a> {
    fn new(bytes: Option<&'a [u8]>) -> Self {
        Side {
            bytes,
            text: None,
            tree: None,
        }
    }
    fn text(&mut self) -> Option<&str> {
        let b = self.bytes?;
        if self.text.is_none() {
            self.text = Some(String::from_utf8_lossy(b).into_owned());
        }
        self.text.as_deref()
    }
    fn tree(&mut self, lang: Lang) -> Option<&Tree> {
        let b = self.bytes?;
        if self.tree.is_none() {
            self.tree = Some(parse(lang, b).ok());
        }
        self.tree.as_ref().and_then(|t| t.as_ref())
    }
}

struct Ctx<'p> {
    policy: &'p Policy,
    att: &'p Attribution,
    findings: Vec<Finding>,
}

impl<'p> Ctx<'p> {
    #[expect(clippy::too_many_arguments, reason = "Collects all finding fields in one place")]
    fn push(
        &mut self,
        rule_id: &str,
        kind: &str,
        path: &str,
        ai_action: Action,
        human_action: Action,
        reason: Option<String>,
        change: &str,
        line: Option<usize>,
        symbol: Option<String>,
        message: String,
    ) {
        let ai = self.att.is_ai(path);
        let action = if ai { ai_action } else { human_action };
        if action == Action::Allow {
            return;
        }
        self.findings.push(Finding {
            rule: rule_id.to_string(),
            kind: kind.to_string(),
            path: path.to_string(),
            line,
            symbol,
            change: change.to_string(),
            message,
            reason,
            ai,
            action,
        });
    }

    fn rule_hit(
        &mut self,
        r: &Rule,
        path: &str,
        change: ChangeKind,
        line: Option<usize>,
        symbol: Option<String>,
        message: String,
    ) {
        if !r.triggers_on(change) {
            return;
        }
        self.push(
            &r.id,
            r.kind,
            path,
            r.action,
            r.human_action,
            r.reason.clone(),
            change_name(change),
            line,
            symbol,
            message,
        );
    }
}

fn change_name(c: ChangeKind) -> &'static str {
    match c {
        ChangeKind::Added => "added",
        ChangeKind::Modified => "modified",
        ChangeKind::Deleted => "deleted",
    }
}

fn file_change_kind(s: Status) -> ChangeKind {
    match s {
        Status::Added => ChangeKind::Added,
        Status::Modified => ChangeKind::Modified,
        Status::Deleted => ChangeKind::Deleted,
    }
}

fn is_binary(b: &[u8]) -> bool {
    b.iter().take(8000).any(|c| *c == 0)
}

type DiffLines = Vec<(usize, String)>;

/// Line-level diff: (added lines with head line numbers, removed lines with base line numbers).
fn line_diff(old: &str, new: &str) -> (DiffLines, DiffLines) {
    let diff = TextDiff::configure()
        .timeout(Duration::from_secs(5))
        .diff_lines(old, new);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for ch in diff.iter_all_changes() {
        let v = ch.value().trim_end_matches(['\n', '\r']).to_string();
        match ch.tag() {
            ChangeTag::Insert => added.push((ch.new_index().unwrap_or(0) + 1, v)),
            ChangeTag::Delete => removed.push((ch.old_index().unwrap_or(0) + 1, v)),
            ChangeTag::Equal => {}
        }
    }
    (added, removed)
}

fn truncate(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(n).collect::<String>())
    }
}

impl Evaluation {
    pub fn exit_code(&self) -> i32 {
        if self.enforced { self.verdict.exit_code() } else { 0 }
    }
}

pub fn evaluate(policy: &Policy, changes: &[FileChange], att: &Attribution) -> Evaluation {
    let mut cx = Ctx {
        policy,
        att,
        findings: Vec::new(),
    };
    let mut lines_changed = 0usize;
    let mut deleted_files = 0usize;
    let mut notes = policy.notes.clone();

    for fc in changes {
        let path = fc.path.as_str();
        let kind = file_change_kind(fc.status);
        if fc.status == Status::Deleted {
            deleted_files += 1;
        }
        let mut old = Side::new(fc.old.as_deref());
        let mut new = Side::new(fc.new.as_deref());
        let old_binary = fc.old.as_deref().map(is_binary).unwrap_or(false);
        let binary = old_binary || fc.new.as_deref().map(is_binary).unwrap_or(false);
        let is_link = fc.new_mode == MODE_SYMLINK || fc.old_mode == MODE_SYMLINK;
        // "Binary" never switches checks off: a single NUL byte in a comment must
        // not be a way around content, symbol or marker rules. Text is read lossily.
        let textual = !is_link && (fc.old.is_some() || fc.new.is_some());

        // Line diff (shared by content rules and limits).
        let (added, removed) = if textual {
            line_diff(old.text().unwrap_or(""), new.text().unwrap_or(""))
        } else {
            (Vec::new(), Vec::new())
        };
        if !binary {
            lines_changed += added.len() + removed.len();
        }

        for r in &policy.rules {
            if !r.covers(path) {
                continue;
            }
            match &r.spec {
                RuleSpec::Path => {
                    let msg = format!("{} protected path", change_name(kind));
                    cx.rule_hit(r, path, kind, None, None, msg);
                }
                RuleSpec::Mentions { re } => {
                    let before = old.text().map(|t| re.is_match(t)).unwrap_or(false);
                    let after = new.text().map(|t| re.is_match(t)).unwrap_or(false);
                    if before || after {
                        let what = match (before, after) {
                            (true, false) => "removes twokey from",
                            (false, true) => "adds twokey to",
                            _ => "changes",
                        };
                        cx.rule_hit(r, path, kind, None, None, format!("{what} a file that wires in twokey"));
                    }
                }
                RuleSpec::Content { re, lines } => {
                    if !textual {
                        continue;
                    }
                    let mut n = 0;
                    let sets: Vec<(&DiffLines, ChangeKind, &str)> = match lines {
                        LineSel::Added => vec![(&added, ChangeKind::Added, "added")],
                        LineSel::Removed => vec![(&removed, ChangeKind::Deleted, "removed")],
                        LineSel::Both => vec![
                            (&added, ChangeKind::Added, "added"),
                            (&removed, ChangeKind::Deleted, "removed"),
                        ],
                    };
                    for (set, ck, verb) in sets {
                        for (ln, l) in set {
                            if re.is_match(l) {
                                n += 1;
                                if n > MAX_HITS_PER_RULE_FILE {
                                    break;
                                }
                                let line = if ck == ChangeKind::Added { Some(*ln) } else { None };
                                let msg = format!("{verb} line {ln} matches /{}/: `{}`", re.as_str(), truncate(l, 80));
                                // Content rules are line-based; ignore `on` filtering by mapping
                                // removed lines to "deleted" and added lines to "added".
                                // `on` for content rules: added lines count as "added",
                                // removed lines as "deleted"; "modified" enables both.
                                if r.triggers_on(ck) || r.on.contains(&ChangeKind::Modified) {
                                    cx.push(
                                        &r.id,
                                        r.kind,
                                        path,
                                        r.action,
                                        r.human_action,
                                        r.reason.clone(),
                                        verb,
                                        line,
                                        None,
                                        msg,
                                    );
                                }
                            }
                        }
                    }
                }
                RuleSpec::Symbol { names } => {
                    let Some(lang) = Lang::from_path(path) else { continue };
                    if !check_size(&mut cx, r, fc, policy.settings.max_parse_bytes) {
                        continue;
                    }
                    let base_syms = side_symbols(&mut old, lang);
                    let head_syms = side_symbols(&mut new, lang);
                    let matches = |s: &Symbol| names.is_match(&s.qname) || names.is_match(&s.name);
                    let b: Vec<&Symbol> = base_syms.iter().filter(|s| matches(s)).collect();
                    let h: Vec<&Symbol> = head_syms.iter().filter(|s| matches(s)).collect();
                    if parse_regressed(&mut old, &mut new, lang) && !b.is_empty() {
                        cx.rule_hit(
                            r,
                            path,
                            ChangeKind::Modified,
                            None,
                            None,
                            "head version has syntax errors, so protected symbols can't be verified".into(),
                        );
                        continue;
                    }
                    compare_symbols(&mut cx, r, path, &b, &h);
                }
                RuleSpec::Query { queries } => {
                    let Some(lang) = Lang::from_path(path) else { continue };
                    for (qlang, q, idx) in queries {
                        if *qlang != lang {
                            continue;
                        }
                        if !check_size(&mut cx, r, fc, policy.settings.max_parse_bytes) {
                            continue;
                        }
                        let bc = side_captures(&mut old, lang, q, *idx);
                        let hc = side_captures(&mut new, lang, q, *idx);
                        if parse_regressed(&mut old, &mut new, lang) {
                            cx.rule_hit(
                                r,
                                path,
                                ChangeKind::Modified,
                                None,
                                None,
                                "head version has syntax errors, so protected constructs can't be verified".into(),
                            );
                            continue;
                        }
                        compare_captures(&mut cx, r, path, &bc, &hc);
                    }
                }
            }
        }

        // Symlinks: a link that points into a protected path is a way to edit it
        // indirectly, so evaluate the target against path-style rules too.
        if fc.new_mode == MODE_SYMLINK && fc.status != Status::Deleted {
            let target = new.text().unwrap_or("").to_string();
            let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let joined = if target.starts_with('/') {
                None
            } else {
                normalize_rel(&format!("{dir}/{target}"))
            };
            let changed_target = fc.old_mode != MODE_SYMLINK || fc.old != fc.new;
            if changed_target {
                match &joined {
                    None => cx.push(
                        "twokey-symlink",
                        "limit",
                        path,
                        policy.limits.symlinks.max(Action::Deny),
                        policy.limits.human_action,
                        None,
                        "added",
                        None,
                        None,
                        format!("symlink points outside the repository (`{target}`)"),
                    ),
                    Some(t) => {
                        for r in policy
                            .rules
                            .iter()
                            // Any rule protecting the target file (or, for a link to a
                            // directory, anything under it) applies to the link itself.
                            .filter(|r| {
                                !matches!(r.spec, RuleSpec::Content { .. })
                                    && (r.covers(t) || r.covers(&format!("{t}/x")))
                            })
                        {
                            cx.push(
                                &r.id,
                                "symlink",
                                path,
                                r.action,
                                r.human_action,
                                r.reason.clone(),
                                "added",
                                None,
                                None,
                                format!("symlink targets protected path `{t}`"),
                            );
                        }
                        cx.push(
                            "twokey-symlink",
                            "limit",
                            path,
                            policy.limits.symlinks,
                            policy.limits.human_action,
                            None,
                            "added",
                            None,
                            None,
                            format!("adds or retargets symlink -> `{target}`"),
                        );
                    }
                }
            }
        }
        if fc.old_mode == MODE_SUBMODULE || fc.new_mode == MODE_SUBMODULE {
            cx.push(
                "twokey-submodule",
                "limit",
                path,
                policy.limits.submodules,
                policy.limits.human_action,
                None,
                change_name(kind),
                None,
                None,
                "changes a git submodule".into(),
            );
        }
        if fc.status == Status::Modified
            && fc.old_mode != fc.new_mode
            && fc.old_mode != MODE_SYMLINK
            && fc.new_mode != MODE_SYMLINK
            && fc.old_mode != MODE_SUBMODULE
            && fc.new_mode != MODE_SUBMODULE
        {
            cx.push(
                "twokey-mode",
                "limit",
                path,
                policy.limits.mode_changes,
                policy.limits.human_action,
                None,
                "modified",
                None,
                None,
                format!("file mode {:o} -> {:o}", fc.old_mode, fc.new_mode),
            );
        } else if fc.status == Status::Added && fc.new_mode == MODE_EXEC {
            cx.push(
                "twokey-mode",
                "limit",
                path,
                policy.limits.mode_changes,
                policy.limits.human_action,
                None,
                "added",
                None,
                None,
                "adds an executable file".into(),
            );
        }
        if binary && fc.new.is_some() {
            cx.push(
                "twokey-binary",
                "limit",
                path,
                policy.limits.binary,
                policy.limits.human_action,
                None,
                change_name(kind),
                None,
                None,
                "binary content can't be reviewed by twokey".into(),
            );
        }

        // Markers are read from the base version; whether they apply depends only
        // on the base side (a head that became a symlink or "binary" can't opt out).
        if policy.markers.enabled
            && fc.old_mode != MODE_SYMLINK
            && !old_binary
            && let Some(base_text) = old.text().map(str::to_string)
        {
            let head_text = if fc.new_mode == MODE_SYMLINK {
                None
            } else {
                new.text().map(str::to_string)
            };
            for hit in markers::compare(&base_text, head_text.as_deref()) {
                let ai_action = policy.markers.lock_action;
                let reason = (!hit.reason.is_empty()).then(|| hit.reason.clone());
                cx.push(
                    &format!("twokey-{}-marker", hit.kind.name()),
                    "marker",
                    path,
                    ai_action,
                    policy.markers.human_action,
                    reason,
                    hit.change,
                    hit.line,
                    None,
                    hit.message,
                );
            }
        }
    }

    // Size limits.
    let lim = &policy.limits;
    if let Some(max) = lim.max_files
        && changes.len() > max
    {
        push_limit(
            &mut cx,
            &format!("changes {} files (limit {max})", changes.len()),
            changes,
        );
    }
    if let Some(max) = lim.max_changed_lines
        && lines_changed > max
    {
        push_limit(
            &mut cx,
            &format!("changes {lines_changed} lines (limit {max})"),
            changes,
        );
    }
    if let Some(max) = lim.max_deleted_files
        && deleted_files > max
    {
        push_limit(
            &mut cx,
            &format!("deletes {deleted_files} files (limit {max})"),
            changes,
        );
    }

    let mut findings = dedup(cx.findings);
    findings.sort_by(|a, b| {
        b.action
            .cmp(&a.action)
            .then(a.path.cmp(&b.path))
            .then(a.line.cmp(&b.line))
    });

    let verdict = if findings.iter().any(|f| f.action == Action::Deny) {
        Verdict::Blocked
    } else {
        Verdict::Pass
    };
    if !policy.settings.enforce {
        notes.push("report-only mode (settings.enforce = false): nothing is blocked".into());
    }
    Evaluation {
        verdict,
        enforced: policy.settings.enforce,
        policy: policy.source.clone(),
        files_changed: changes.len(),
        lines_changed,
        attribution: att.clone(),
        findings,
        notes,
    }
}

fn push_limit(cx: &mut Ctx, msg: &str, changes: &[FileChange]) {
    // Size limits apply to the change as a whole: AI if any changed file is AI.
    let ai = changes.iter().any(|c| cx.att.is_ai(&c.path));
    let lim = &cx.policy.limits;
    let action = if ai { lim.action } else { lim.human_action };
    if action == Action::Allow {
        return;
    }
    cx.findings.push(Finding {
        rule: "twokey-limits".into(),
        kind: "limit".into(),
        path: String::new(),
        line: None,
        symbol: None,
        change: "size".into(),
        message: msg.to_string(),
        reason: Some("Large AI changes are hard to review; split them up.".into()),
        ai,
        action,
    });
}

fn check_size(cx: &mut Ctx, r: &Rule, fc: &FileChange, max: usize) -> bool {
    let too_big = fc.old.as_ref().map(|b| b.len() > max).unwrap_or(false)
        || fc.new.as_ref().map(|b| b.len() > max).unwrap_or(false);
    if too_big {
        cx.rule_hit(
            r,
            &fc.path,
            ChangeKind::Modified,
            None,
            None,
            format!("file exceeds max_parse_bytes ({max}); can't verify protected code"),
        );
    }
    !too_big
}

fn side_symbols(s: &mut Side, lang: Lang) -> Vec<Symbol> {
    let Some(bytes) = s.bytes else { return Vec::new() };
    match s.tree(lang) {
        Some(t) => symbols::extract(lang, t, bytes),
        None => Vec::new(),
    }
}

fn side_captures(s: &mut Side, lang: Lang, q: &tree_sitter::Query, idx: u32) -> Vec<Capture> {
    let Some(bytes) = s.bytes else { return Vec::new() };
    match s.tree(lang) {
        Some(t) => symbols::captures(q, idx, t, bytes),
        None => Vec::new(),
    }
}

/// True if the head has parse errors that the base didn't (fail closed).
fn parse_regressed(old: &mut Side, new: &mut Side, lang: Lang) -> bool {
    let has_head = new.bytes.is_some();
    let head_err = match new.tree(lang) {
        Some(t) => t.root_node().has_error(),
        None => has_head,
    };
    let base_err = old.tree(lang).map(|t| t.root_node().has_error()).unwrap_or(false);
    head_err && !base_err
}

fn compare_symbols(cx: &mut Ctx, r: &Rule, path: &str, base: &[&Symbol], head: &[&Symbol]) {
    let mut by: BTreeMap<&str, (Vec<&Symbol>, Vec<&Symbol>)> = BTreeMap::new();
    for s in base {
        by.entry(s.qname.as_str()).or_default().0.push(s);
    }
    for s in head {
        by.entry(s.qname.as_str()).or_default().1.push(s);
    }
    for (q, (b, h)) in by {
        for i in 0..b.len().max(h.len()) {
            match (b.get(i), h.get(i)) {
                (Some(x), Some(y)) if x.fingerprint != y.fingerprint => cx.rule_hit(
                    r,
                    path,
                    ChangeKind::Modified,
                    Some(y.line),
                    Some(q.to_string()),
                    format!("modified protected {} `{q}`", y.kind),
                ),
                (Some(x), None) => cx.rule_hit(
                    r,
                    path,
                    ChangeKind::Deleted,
                    None,
                    Some(q.to_string()),
                    format!(
                        "deleted or renamed protected {} `{q}` (was at base line {})",
                        x.kind, x.line
                    ),
                ),
                // A second definition of an existing name shadows/overrides the
                // original, so it's a modification, not a harmless addition.
                (None, Some(y)) if b.is_empty() => cx.rule_hit(
                    r,
                    path,
                    ChangeKind::Added,
                    Some(y.line),
                    Some(q.to_string()),
                    format!("added {} `{q}` matching a protected name", y.kind),
                ),
                (None, Some(y)) => cx.rule_hit(
                    r,
                    path,
                    ChangeKind::Modified,
                    Some(y.line),
                    Some(q.to_string()),
                    format!("redefined protected {} `{q}` (shadows the original)", y.kind),
                ),
                _ => {}
            }
        }
    }
}

fn compare_captures(cx: &mut Ctx, r: &Rule, path: &str, base: &[Capture], head: &[Capture]) {
    // Multiset difference by fingerprint.
    let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
    for c in base {
        *counts.entry(c.fingerprint.as_str()).or_default() += 1;
    }
    let mut added: Vec<&Capture> = Vec::new();
    for c in head {
        let e = counts.entry(c.fingerprint.as_str()).or_default();
        if *e > 0 {
            *e -= 1;
        } else {
            added.push(c);
        }
    }
    let mut removed: Vec<&Capture> = Vec::new();
    let mut left: BTreeMap<&str, i64> = counts.into_iter().filter(|(_, n)| *n > 0).collect();
    for c in base {
        if let Some(n) = left.get_mut(c.fingerprint.as_str())
            && *n > 0
        {
            *n -= 1;
            removed.push(c);
        }
    }
    // Pair removed+added as modifications, leftovers are plain adds/deletes.
    let paired = added.len().min(removed.len());
    let mut n = 0;
    for (i, a) in added.iter().enumerate() {
        n += 1;
        if n > MAX_HITS_PER_RULE_FILE {
            break;
        }
        if i < paired {
            cx.rule_hit(
                r,
                path,
                ChangeKind::Modified,
                Some(a.line),
                None,
                format!(
                    "modified protected construct `{}` (base line {})",
                    a.preview, removed[i].line
                ),
            );
        } else {
            cx.rule_hit(
                r,
                path,
                ChangeKind::Added,
                Some(a.line),
                None,
                format!("added protected construct `{}`", a.preview),
            );
        }
    }
    for d in removed.iter().skip(paired).take(MAX_HITS_PER_RULE_FILE) {
        cx.rule_hit(
            r,
            path,
            ChangeKind::Deleted,
            None,
            None,
            format!("removed protected construct `{}` (base line {})", d.preview, d.line),
        );
    }
}

fn dedup(v: Vec<Finding>) -> Vec<Finding> {
    let mut seen = HashSet::new();
    v.into_iter()
        .filter(|f| {
            seen.insert((
                f.rule.clone(),
                f.path.clone(),
                f.change.clone(),
                f.symbol.clone(),
                f.line,
                f.message.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Policy;

    fn fc(path: &str, old: Option<&str>, new: Option<&str>) -> FileChange {
        FileChange {
            path: path.into(),
            status: match (old, new) {
                (None, _) => Status::Added,
                (_, None) => Status::Deleted,
                _ => Status::Modified,
            },
            old_mode: if old.is_some() { 0o100644 } else { 0 },
            new_mode: if new.is_some() { 0o100644 } else { 0 },
            old: old.map(|s| s.as_bytes().to_vec()),
            new: new.map(|s| s.as_bytes().to_vec()),
        }
    }

    const POLICY: &str = r#"

[[rule]]
id = "workflows"
kind = "path"
paths = [".github/workflows/**"]
action = "deny"

[[rule]]
id = "auth"
kind = "symbol"
paths = ["src/auth/**"]
symbols = ["verify_*"]
action = "deny"

[[rule]]
id = "unsafe"
kind = "query"
language = "rust"
query = "(unsafe_block) @protect"
on = ["added", "modified"]

[[rule]]
id = "skips"
kind = "content"
paths = ["*.py"]
pattern = '@pytest\.mark\.skip'
action = "deny"
"#;

    fn run(changes: Vec<FileChange>, ai: bool) -> Evaluation {
        let p = Policy::compile(POLICY, ".twokey.toml", "test".into()).unwrap();
        let att = Attribution::whole(ai, "test");
        evaluate(&p, &changes, &att)
    }

    #[test]
    fn twokey_config_is_always_protected() {
        // Policy with no rules at all: built-ins still apply.
        let p = Policy::compile("", ".twokey.toml", "t".into()).unwrap();
        let run = |c: FileChange, ai: bool| evaluate(&p, &[c], &Attribution::whole(ai, "t"));
        let hook = "#!/bin/sh\nexec twokey check --staged\n";
        // editing, removing twokey from, or deleting a file that wires it in
        assert_eq!(
            run(
                fc(".githooks/pre-commit", Some(hook), Some("#!/bin/sh\nexit 0\n")),
                true
            )
            .verdict,
            Verdict::Blocked
        );
        assert_eq!(
            run(fc(".githooks/pre-commit", Some(hook), None), true).verdict,
            Verdict::Blocked
        );
        assert_eq!(
            run(
                fc(".github/workflows/x.yml", Some("a"), Some("run: twokey check")),
                true
            )
            .verdict,
            Verdict::Blocked
        );
        // Human edits are governed by the repository, not twokey approvals.
        assert_eq!(
            run(fc(".githooks/pre-commit", Some(hook), Some("x")), false).verdict,
            Verdict::Pass,
        );
        // unrelated workflows and hooks are not affected by the built-in
        assert_eq!(
            run(fc(".github/workflows/lint.yml", Some("a"), Some("b")), true).verdict,
            Verdict::Pass
        );
        // every policy file anywhere, the runner script, the .twokey dir
        for f in [
            "services/pay/.twokey.toml",
            ".github/twokey/run.sh",
            ".twokey/config.json",
        ] {
            assert_eq!(run(fc(f, Some("a"), Some("b")), true).verdict, Verdict::Blocked, "{f}");
        }
    }

    #[test]
    fn deny_is_reported_but_not_enforced_in_dry_run() {
        let mut p = Policy::compile(POLICY, ".twokey.toml", "test".into()).unwrap();
        p.settings.enforce = false;
        let e = evaluate(
            &p,
            &[fc("src/auth/mod.rs", Some("fn verify_x() {}"), None)],
            &Attribution::whole(true, "test"),
        );
        assert_eq!(e.verdict, Verdict::Blocked);
        assert_eq!(e.exit_code(), 0);
    }

    #[test]
    fn path_deny_for_ai_only() {
        let c = vec![fc(".github/workflows/ci.yml", Some("a"), Some("b"))];
        assert_eq!(run(c.clone(), true).verdict, Verdict::Blocked);
        assert_eq!(run(c, false).verdict, Verdict::Pass);
    }

    #[test]
    fn policy_file_is_self_protected() {
        let c = vec![fc(".twokey.toml", Some("a"), Some("b"))];
        assert_eq!(run(c.clone(), true).verdict, Verdict::Blocked);
        assert_eq!(run(c, false).verdict, Verdict::Pass);
    }

    #[test]
    fn protected_symbol_blocks_ai_and_allows_humans() {
        let base = "pub fn verify_token(t: &str) -> bool { t.len() == 32 }\npub fn helper() {}\n";
        let head = "pub fn verify_token(t: &str) -> bool { true }\npub fn helper() {}\n";
        let c = vec![fc("src/auth/mod.rs", Some(base), Some(head))];
        let e = run(c.clone(), true);
        assert_eq!(e.verdict, Verdict::Blocked);
        assert_eq!(e.exit_code(), 1);
        assert_eq!(e.findings[0].symbol.as_deref(), Some("verify_token"));
        assert_eq!(run(c, false).verdict, Verdict::Pass);
    }

    #[test]
    fn unrelated_edits_and_reformatting_pass() {
        let base = "pub fn verify_token(t: &str) -> bool { t.len() == 32 }\npub fn helper() {}\n";
        let head =
            "pub fn verify_token(t: &str) -> bool {\n    t.len() == 32\n}\npub fn helper() { println!(\"hi\"); }\n";
        let e = run(vec![fc("src/auth/mod.rs", Some(base), Some(head))], true);
        assert_eq!(e.verdict, Verdict::Pass, "{:?}", e.findings);
    }

    #[test]
    fn renaming_a_protected_symbol_is_caught() {
        let base = "pub fn verify_token() {}\n";
        let head = "pub fn check_token() {}\n";
        let e = run(vec![fc("src/auth/mod.rs", Some(base), Some(head))], true);
        assert!(e.findings.iter().any(|f| f.change == "deleted"));
    }

    #[test]
    fn new_unsafe_block_flagged() {
        let base = "fn f() { let x = 1; }\n";
        let head = "fn f() { let x = unsafe { *std::ptr::null::<u8>() }; }\n";
        let e = run(vec![fc("src/lib.rs", Some(base), Some(head))], true);
        assert_eq!(e.verdict, Verdict::Blocked);
        assert_eq!(e.findings[0].rule, "unsafe");
    }

    #[test]
    fn content_rule_on_added_lines_only() {
        let base = "@pytest.mark.skip\ndef test_a(): pass\n";
        let head = "@pytest.mark.skip\ndef test_a(): pass\n\n@pytest.mark.skip\ndef test_b(): pass\n";
        let e = run(vec![fc("tests/test_x.py", Some(base), Some(head))], true);
        assert_eq!(e.verdict, Verdict::Blocked);
        assert_eq!(e.findings.iter().filter(|f| f.rule == "skips").count(), 1);
        assert_eq!(e.findings[0].line, Some(4));
    }

    #[test]
    fn content_rule_bans_em_dashes() {
        let policy = Policy::compile(&crate::config::tests::enabled_example(), ".twokey.toml", "test".into()).unwrap();
        let check = |before, after, ai| {
            evaluate(
                &policy,
                &[fc("docs/guide.md", before, after)],
                &Attribution::whole(ai, "test"),
            )
        };
        for ai in [true, false] {
            let added = check(None, Some("Heading\nUse this — carefully.\n"), ai);
            assert_eq!(added.verdict, Verdict::Blocked);
            assert_eq!(added.findings[0].rule, "no-em-dashes");
            assert_eq!(added.findings[0].line, Some(2));
            assert_eq!(
                check(Some("Old — text.\n"), Some("New — text.\n"), ai).verdict,
                Verdict::Blocked
            );
            assert_eq!(
                check(Some("Old — text.\n"), Some("Old text.\n"), ai).verdict,
                Verdict::Pass
            );
            assert_eq!(check(Some("Old — text.\n"), None, ai).verdict, Verdict::Pass);
            assert_eq!(
                check(Some("Old — text.\n"), Some("Old — text.\nNew text.\n"), ai).verdict,
                Verdict::Pass
            );
            assert_eq!(
                check(None, Some("Hyphen - and en dash –.\n"), ai).verdict,
                Verdict::Pass
            );
        }
    }

    #[test]
    fn syntax_errors_fail_closed() {
        let base = "pub fn verify_token() -> bool { false }\n";
        let head = "pub fn verify_token() -> bool { false \n";
        let e = run(vec![fc("src/auth/mod.rs", Some(base), Some(head))], true);
        assert_ne!(e.verdict, Verdict::Pass);
    }

    #[test]
    fn symlink_into_protected_path() {
        let mut c = fc("docs/ci.yml", None, Some("../.github/workflows/ci.yml"));
        c.new_mode = MODE_SYMLINK;
        let e = run(vec![c], true);
        assert_eq!(e.verdict, Verdict::Blocked);
    }

    #[test]
    fn lock_markers() {
        let base = "x\n// twokey:lock-begin\nconst K: u8 = 1;\n// twokey:lock-end\n";
        let head = base.replace("= 1", "= 2");
        let e = run(vec![fc("src/k.rs", Some(base), Some(&head))], true);
        assert_eq!(e.verdict, Verdict::Blocked);
    }
}
