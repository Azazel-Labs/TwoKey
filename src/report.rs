//! Output formats: human text, JSON, GitHub annotations, Markdown.

use crate::config::Action;
use crate::engine::{Evaluation, Finding, Verdict};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum Format {
    #[default]
    Text,
    Json,
    Github,
    Markdown,
}

/// Hidden marker so CI can find and update its own PR comment.
pub const MARKDOWN_MARKER: &str = "<!-- twokey-report -->";

pub fn render(e: &Evaluation, f: Format) -> String {
    match f {
        Format::Text => text(e),
        Format::Json => serde_json::to_string_pretty(e).unwrap_or_default() + "\n",
        Format::Markdown => markdown(e),
        Format::Github => {
            let mut s = github_annotations(e);
            // File names can contain newlines; never let report text start a line
            // with `::`, which the runner would execute as a workflow command.
            for line in text(e).lines() {
                // (the runner ignores leading whitespace, so neutralize on trim)
                if line.trim_start().starts_with("::") {
                    s.push_str("| ");
                }
                s.push_str(line);
                s.push('\n');
            }
            if let Ok(p) = std::env::var("GITHUB_STEP_SUMMARY")
                && !p.is_empty()
            {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                    let _ = f.write_all(markdown(e).as_bytes());
                }
            }
            s
        }
    }
}

fn verdict_label(e: &Evaluation) -> String {
    if e.enforced {
        e.verdict.label().to_string()
    } else {
        format!("{} (report-only, not enforced)", e.verdict.label())
    }
}

/// Make PR-controlled text (paths, matched lines) inert inside a Markdown table:
/// no table breaks, no HTML, no @-mentions, no code-span escapes.
fn md_cell(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "\\|")
        .replace('`', "'")
        .replace('@', "@\u{200b}")
        .replace('[', "\\[")
}

fn counts(e: &Evaluation) -> (usize, usize, usize, usize) {
    let deny = e.findings.iter().filter(|f| f.action == Action::Deny).count();
    let pending = e
        .findings
        .iter()
        .filter(|f| f.action == Action::Approve && f.approved_by.is_none())
        .count();
    let approved = e.findings.iter().filter(|f| f.approved_by.is_some()).count();
    let warn = e.findings.iter().filter(|f| f.action == Action::Warn).count();
    (deny, pending, approved, warn)
}

fn location(f: &Finding) -> String {
    let mut s = if f.path.is_empty() {
        "(whole change)".to_string()
    } else {
        f.path.clone()
    };
    if let Some(l) = f.line {
        let _ = write!(s, ":{l}");
    }
    s
}

fn status_label(f: &Finding) -> &'static str {
    match (f.action, f.approved_by.is_some()) {
        (Action::Deny, _) => "DENY",
        (Action::Approve, true) => "APPROVED",
        (Action::Approve, false) => "APPROVE",
        (Action::Warn, _) => "WARN",
        (Action::Allow, _) => "ALLOW",
    }
}

fn attribution_line(e: &Evaluation) -> String {
    let a = &e.attribution;
    if a.all_ai {
        format!("all changes treated as AI ({})", a.reasons.join("; "))
    } else if a.commits.is_empty() {
        format!("not AI-attributed ({})", a.reasons.join("; "))
    } else {
        let n = a.commits.iter().filter(|c| c.ai).count();
        format!("{n} of {} commits AI-attributed", a.commits.len())
    }
}

pub fn text(e: &Evaluation) -> String {
    let (deny, pending, approved, warn) = counts(e);
    let mut s = String::new();
    let _ = writeln!(
        s,
        "twokey: {} — {deny} blocked, {pending} awaiting approval, {approved} approved, {warn} warnings",
        verdict_label(e)
    );
    let _ = writeln!(
        s,
        "  {} files, {} lines changed · {}",
        e.files_changed,
        e.lines_changed,
        attribution_line(e)
    );
    let _ = writeln!(s, "  policy: {}", e.policy);
    for n in &e.notes {
        let _ = writeln!(s, "  note: {n}");
    }
    for c in e.attribution.commits.iter().filter(|c| c.ai) {
        let _ = writeln!(
            s,
            "  ai commit {} {} — {}",
            &c.sha[..c.sha.len().min(10)],
            c.subject,
            c.why.join(", ")
        );
    }
    if !e.findings.is_empty() {
        s.push('\n');
    }
    for f in &e.findings {
        let _ = writeln!(
            s,
            "  {:<9} [{}] {} — {}",
            status_label(f),
            f.rule,
            location(f),
            f.message
        );
        if let Some(r) = &f.reason {
            let _ = writeln!(s, "            {r}");
        }
        if f.action == Action::Approve {
            match &f.approved_by {
                Some(a) => {
                    let _ = writeln!(s, "            approved by {a}");
                }
                None if f.approvers.is_empty() => {
                    let _ = writeln!(s, "            needs approval from any maintainer");
                }
                None => {
                    let _ = writeln!(s, "            needs approval from one of: {}", f.approvers.join(", "));
                }
            }
        }
    }
    s
}

fn gh_escape_data(s: &str) -> String {
    s.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

fn gh_escape_prop(s: &str) -> String {
    gh_escape_data(s).replace(':', "%3A").replace(',', "%2C")
}

pub fn github_annotations(e: &Evaluation) -> String {
    let mut s = String::new();
    for f in &e.findings {
        let level = match (f.action, f.approved_by.is_some()) {
            (Action::Deny, _) | (Action::Approve, false) if e.enforced => "error",
            (Action::Deny, _) | (Action::Approve, false) => "warning",
            (Action::Warn, _) => "warning",
            _ => "notice",
        };
        let mut props = Vec::new();
        if !f.path.is_empty() {
            props.push(format!("file={}", gh_escape_prop(&f.path)));
            if let Some(l) = f.line {
                props.push(format!("line={l}"));
            }
        }
        props.push(format!(
            "title={}",
            gh_escape_prop(&format!("twokey {}: {}", status_label(f), f.rule))
        ));
        let mut msg = f.message.clone();
        if let Some(r) = &f.reason {
            msg.push('\n');
            msg.push_str(r);
        }
        if f.action == Action::Approve && f.approved_by.is_none() && !f.approvers.is_empty() {
            msg.push_str(&format!("\nNeeds approval from: {}", f.approvers.join(", ")));
        }
        let _ = writeln!(s, "::{level} {}::{}", props.join(","), gh_escape_data(&msg));
    }
    s
}

pub fn markdown(e: &Evaluation) -> String {
    let (deny, pending, approved, warn) = counts(e);
    let icon = match e.verdict {
        Verdict::Pass => "✅",
        Verdict::NeedsApproval => "🟡",
        Verdict::Blocked => "⛔",
    };
    let mut s = String::new();
    let _ = writeln!(s, "{MARKDOWN_MARKER}\n### {icon} twokey: {}\n", verdict_label(e));
    let _ = writeln!(
        s,
        "{deny} blocked · {pending} awaiting approval · {approved} approved · {warn} warnings — {} files, {} lines · {}\n",
        e.files_changed,
        e.lines_changed,
        attribution_line(e)
    );
    if !e.findings.is_empty() {
        let _ = writeln!(s, "| | Rule | Where | What | Approvers |");
        let _ = writeln!(s, "|---|---|---|---|---|");
        for f in &e.findings {
            let who = match (&f.approved_by, f.action) {
                (Some(a), _) => format!("✔ {a}"),
                (None, Action::Approve) if f.approvers.is_empty() => "any maintainer".into(),
                (None, Action::Approve) => f.approvers.join(", "),
                _ => String::new(),
            };
            let what = match &f.reason {
                Some(r) => format!("{}<br><sub>{}</sub>", md_cell(&f.message), md_cell(r)),
                None => md_cell(&f.message),
            };
            let _ = writeln!(
                s,
                "| {} | `{}` | `{}` | {} | {} |",
                status_label(f),
                md_cell(&f.rule),
                md_cell(&location(f)),
                what,
                md_cell(&who)
            );
        }
        s.push('\n');
    }
    for n in &e.notes {
        let _ = writeln!(s, "<sub>note: {}</sub><br>", md_cell(n));
    }
    let _ = writeln!(s, "<sub>policy: {}</sub>\n", md_cell(&e.policy));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_cells_are_inert() {
        let c = md_cell("a|b\n::error x <img src=y> @org/team [x](http://e) `c`");
        assert!(!c.contains('\n') && !c.contains("<img") && !c.contains("@org") && !c.contains('`'));
        assert!(c.contains("\\|"));
    }
}
