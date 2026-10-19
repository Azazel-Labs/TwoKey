//! `twokey scan`: show what the policy protects in a given revision, so you can
//! check coverage before relying on it ("is `verify_token` actually matched?").

use crate::config::{Policy, RuleSpec};
use crate::git::{BlobReader, MODE_SUBMODULE, Repo};
use crate::lang::{Lang, parse};
use crate::markers;
use crate::symbols;
use anyhow::Result;
use serde::Serialize;
use std::fmt::Write as _;

#[derive(Serialize)]
pub struct Protection {
    pub path: String,
    pub rule: String,
    pub action: String,
    pub what: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

pub fn scan(repo: &Repo, policy: &Policy, rev: &str) -> Result<Vec<Protection>> {
    let mut out = Vec::new();
    let mut reader = BlobReader::new(repo)?;
    for (mode, sha, path) in repo.ls_tree(rev)? {
        if mode == MODE_SUBMODULE {
            continue;
        }
        let mut bytes: Option<Vec<u8>> = None;
        let mut load = |_: &Repo| -> Option<Vec<u8>> {
            if bytes.is_none() {
                bytes = reader.read(&sha).ok();
            }
            bytes.clone()
        };
        for r in &policy.rules {
            if !r.covers(&path) {
                continue;
            }
            let act = format!("{:?}/{:?}", r.action, r.human_action).to_lowercase();
            match &r.spec {
                RuleSpec::Path => out.push(Protection {
                    path: path.clone(),
                    rule: r.id.clone(),
                    action: act,
                    what: "whole file".into(),
                    line: None,
                }),
                RuleSpec::Mentions { re } => {
                    let Some(b) = load(repo) else { continue };
                    if re.is_match(&String::from_utf8_lossy(&b)) {
                        out.push(Protection {
                            path: path.clone(),
                            rule: r.id.clone(),
                            action: act,
                            what: "whole file (wires in twokey)".into(),
                            line: None,
                        });
                    }
                }
                RuleSpec::Content { re, .. } => out.push(Protection {
                    path: path.clone(),
                    rule: r.id.clone(),
                    action: act,
                    what: format!("new lines matching /{}/", re.as_str()),
                    line: None,
                }),
                RuleSpec::Symbol { names } => {
                    let Some(lang) = Lang::from_path(&path) else { continue };
                    let Some(b) = load(repo) else { continue };
                    if b.len() > policy.settings.max_parse_bytes {
                        continue;
                    }
                    let Ok(tree) = parse(lang, &b) else { continue };
                    for s in symbols::extract(lang, &tree, &b) {
                        if names.is_match(&s.qname) || names.is_match(&s.name) {
                            out.push(Protection {
                                path: path.clone(),
                                rule: r.id.clone(),
                                action: act.clone(),
                                what: format!("{} {}", s.kind, s.qname),
                                line: Some(s.line),
                            });
                        }
                    }
                }
                RuleSpec::Query { queries } => {
                    let Some(lang) = Lang::from_path(&path) else { continue };
                    let Some(b) = load(repo) else { continue };
                    for (ql, q, idx) in queries {
                        if *ql != lang {
                            continue;
                        }
                        let Ok(tree) = parse(lang, &b) else { continue };
                        for c in symbols::captures(q, *idx, &tree, &b) {
                            out.push(Protection {
                                path: path.clone(),
                                rule: r.id.clone(),
                                action: act.clone(),
                                what: format!("`{}`", c.preview),
                                line: Some(c.line),
                            });
                        }
                    }
                }
            }
        }
        if policy.markers.enabled
            && let Some(b) = load(repo)
            && !b.iter().take(8000).any(|c| *c == 0)
        {
            let s = markers::scan(&String::from_utf8_lossy(&b));
            for (k, ln, reason) in s.file {
                out.push(Protection {
                    path: path.clone(),
                    rule: format!("twokey-{}-marker", k.name()),
                    action: k.name().into(),
                    what: format!(
                        "whole file{}",
                        if reason.is_empty() {
                            String::new()
                        } else {
                            format!(" — {reason}")
                        }
                    ),
                    line: Some(ln),
                });
            }
            for r in s.regions {
                out.push(Protection {
                    path: path.clone(),
                    rule: format!("twokey-{}-marker", r.kind.name()),
                    action: r.kind.name().into(),
                    what: format!(
                        "lines {}-{}{}",
                        r.begin_line,
                        r.end_line.map(|e| e.to_string()).unwrap_or("EOF".into()),
                        if r.reason.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", r.reason)
                        }
                    ),
                    line: Some(r.begin_line),
                });
            }
        }
    }
    Ok(out)
}

pub fn render_text(items: &[Protection]) -> String {
    let mut s = String::new();
    let mut last = "";
    for p in items {
        if p.path != last {
            let _ = writeln!(s, "{}", p.path);
            last = &p.path;
        }
        let line = p.line.map(|l| format!(":{l}")).unwrap_or_default();
        let _ = writeln!(s, "    {:<28} {:<18} {}{}", p.rule, p.action, p.what, line);
    }
    if items.is_empty() {
        s.push_str("nothing in this revision is protected by the policy\n");
    }
    s
}
