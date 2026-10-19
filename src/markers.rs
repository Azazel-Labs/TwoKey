//! In-file protection markers, language-agnostic (they live in comments):
//!
//! ```text
//! // twokey:lock-begin  reason text      (AI may never change this region)
//! // twokey:lock-end
//! # twokey:review-begin reason text      (AI changes need human approval)
//! # twokey:review-end
//! /* twokey:lock-file */                  (whole file)
//! ```
//!
//! Markers are always read from the *base* version, so deleting a marker is itself
//! a change to the protected region rather than a way around it.

use regex::Regex;
use similar::{DiffTag, TextDiff};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Lock,
    Review,
}

impl MarkerKind {
    pub fn name(self) -> &'static str {
        match self {
            MarkerKind::Lock => "lock",
            MarkerKind::Review => "review",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Region {
    pub kind: MarkerKind,
    pub begin_line: usize,
    pub end_line: Option<usize>,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct Scan {
    pub file: Vec<(MarkerKind, usize, String)>,
    pub regions: Vec<Region>,
}

fn marker_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // A marker must be the first thing in a comment line, so prose or string
    // literals that merely mention a marker are not markers.
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?://+!?|#+|/\*+|\*+|--|<!--|\{#|;+|%+|'|rem\s)?\s*twokey:(lock|review)-(begin|end|file)\b(.*)$",
        )
        .unwrap()
    })
}

fn clean_reason(s: &str) -> String {
    s.trim()
        .trim_end_matches("*/")
        .trim_end_matches("-->")
        .trim_end_matches("#}")
        .trim()
        .trim_start_matches([':', '-', '—'])
        .trim()
        .to_string()
}

pub fn scan(text: &str) -> Scan {
    let mut s = Scan::default();
    let mut open: Vec<Region> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let ln = i + 1;
        if let Some(c) = marker_re().captures(line) {
            let kind = if c[1].eq_ignore_ascii_case("lock") {
                MarkerKind::Lock
            } else {
                MarkerKind::Review
            };
            let reason = clean_reason(c.get(3).map(|m| m.as_str()).unwrap_or(""));
            match c[2].to_ascii_lowercase().as_str() {
                "file" => s.file.push((kind, ln, reason)),
                "begin" => open.push(Region {
                    kind,
                    begin_line: ln,
                    end_line: None,
                    reason,
                }),
                _ => {
                    // close the innermost open region of the same kind
                    if let Some(pos) = open.iter().rposition(|r| r.kind == kind) {
                        let mut r = open.remove(pos);
                        r.end_line = Some(ln);
                        s.regions.push(r);
                    }
                }
            }
            continue;
        }
    }
    // Unterminated regions extend to end of file (fail closed).
    s.regions.extend(open);
    s.regions.sort_by_key(|r| r.begin_line);
    s
}

#[derive(Debug, Clone)]
pub struct MarkerHit {
    pub kind: MarkerKind,
    pub change: &'static str,
    pub line: Option<usize>,
    pub message: String,
    pub reason: String,
}

/// Compare base vs head given the base-side markers.
pub fn compare(base: &str, head: Option<&str>) -> Vec<MarkerHit> {
    let b = scan(base);
    let mut hits = Vec::new();
    if b.file.is_empty() && b.regions.is_empty() {
        return hits;
    }
    let changed = head.map(|h| h != base).unwrap_or(true);
    for (kind, ln, reason) in &b.file {
        if changed {
            hits.push(MarkerHit {
                kind: *kind,
                change: if head.is_none() { "deleted" } else { "modified" },
                line: None,
                message: format!(
                    "{} {}-protected file (marker at base line {ln})",
                    if head.is_none() { "deleted" } else { "modified" },
                    kind.name()
                ),
                reason: reason.clone(),
            });
        }
    }
    let Some(head) = head else {
        for r in &b.regions {
            hits.push(MarkerHit {
                kind: r.kind,
                change: "deleted",
                line: None,
                message: format!(
                    "deleted file containing {}-protected region (base lines {}-{})",
                    r.kind.name(),
                    r.begin_line,
                    r.end_line.map(|e| e.to_string()).unwrap_or("EOF".into())
                ),
                reason: r.reason.clone(),
            });
        }
        return hits;
    };
    // Anchor regions to the base and look at what the line diff does to them.
    // (Pairing regions by position in the head would let a decoy copy of a
    // region, inserted earlier in the file, stand in for the real one.)
    // Trailing whitespace is ignored; leading indentation is significant
    // (YAML, Python).
    let norm = |s: &str| s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") + "\n";
    let (nb, nh) = (norm(base), norm(head));
    let diff = TextDiff::from_lines(&nb, &nh);
    let ops = diff.ops();
    let base_lines = nb.lines().count();
    let head_lines: Vec<&str> = nh.lines().collect();
    for r in &b.regions {
        // 0-based, inclusive of both marker lines: [lo, hi)
        let lo = r.begin_line - 1;
        let hi = r.end_line.unwrap_or(base_lines).max(r.begin_line);
        let range = format!(
            "base lines {}-{}",
            r.begin_line,
            r.end_line.map(|e| e.to_string()).unwrap_or("EOF".into())
        );
        let mut hit: Option<(&'static str, Option<usize>)> = None;
        for op in ops {
            let (o, n) = (op.old_range(), op.new_range());
            let touches = match op.tag() {
                DiffTag::Equal => false,
                // Insertion before old line `o.start`: inside if strictly after the
                // begin marker and not after the end marker.
                DiffTag::Insert => {
                    let inside = o.start > lo && o.start < hi;
                    // Lines inserted right next to the markers that could neutralize
                    // the region (wrap it in a comment, string or dead cfg block).
                    let adjacent = (o.start == lo || o.start == hi)
                        && head_lines[n.clone()].iter().any(|l| neutralizer_re().is_match(l));
                    inside || adjacent
                }
                DiffTag::Delete | DiffTag::Replace => o.start < hi && o.end > lo,
            };
            if touches {
                let removed_all = op.tag() == DiffTag::Delete && o.start <= lo && o.end >= hi;
                hit = Some((if removed_all { "deleted" } else { "modified" }, Some(n.start + 1)));
                break;
            }
        }
        if let Some((change, line)) = hit {
            hits.push(MarkerHit {
                kind: r.kind,
                change,
                line: if change == "deleted" { None } else { line },
                message: format!(
                    "{} {}-protected region or its markers ({range})",
                    if change == "deleted" { "removed" } else { "modified" },
                    r.kind.name()
                ),
                reason: r.reason.clone(),
            });
        }
    }
    hits
}

fn neutralizer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(/\*|\*/|<!--|-->|"""|'''|`|^\s*#\s*if\b|^\s*#\s*\[\s*cfg|\bif\s*\(?\s*(false|0)\b|^\s*=begin|^\s*=end)"#)
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "a\n// twokey:lock-begin crypto constants\nconst K: u32 = 7;\n// twokey:lock-end\nb\n";

    #[test]
    fn untouched_region_is_fine() {
        let head = "a changed\n// twokey:lock-begin crypto constants\nconst K: u32 = 7;   \n// twokey:lock-end\nb\nc\n";
        assert!(compare(BASE, Some(head)).is_empty());
    }

    #[test]
    fn edited_region_is_caught() {
        let head = BASE.replace("= 7", "= 8");
        let h = compare(BASE, Some(&head));
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].change, "modified");
        assert_eq!(h[0].reason, "crypto constants");
    }

    #[test]
    fn removing_markers_is_caught() {
        let head = "a\nconst K: u32 = 7;\nb\n";
        let h = compare(BASE, Some(head));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn decoy_region_does_not_hide_the_real_one() {
        let decoy = "/*\n// twokey:lock-begin crypto constants\nconst K: u32 = 7;\n// twokey:lock-end\n*/\n";
        let head = format!("{decoy}{}", BASE.replace("= 7", "= 8"));
        assert_eq!(compare(BASE, Some(&head)).len(), 1);
    }

    #[test]
    fn indentation_inside_region_matters() {
        let base = "# twokey:lock-begin\npermissions:\n  contents: read\n# twokey:lock-end\n";
        let head = "# twokey:lock-begin\npermissions:\ncontents: read\n# twokey:lock-end\n";
        assert_eq!(compare(base, Some(head)).len(), 1);
    }

    #[test]
    fn commenting_out_a_region_is_caught() {
        let head = BASE.replace("a\n", "a\n/*\n").replace("lock-end\n", "lock-end\n*/\n");
        assert_eq!(compare(BASE, Some(&head)).len(), 1);
    }

    #[test]
    fn whole_file_lock() {
        let base = "# twokey:review-file schema owned by data team\nx = 1\n";
        assert!(compare(base, Some(base)).is_empty());
        assert_eq!(compare(base, Some("x = 2\n")).len(), 1);
        assert_eq!(compare(base, None).len(), 1);
    }
}
