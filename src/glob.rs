//! Path and name glob helpers with gitignore-like conventions.

use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// Normalize a path pattern:
/// * `/foo/**`  -> anchored at repo root (`foo/**`)
/// * `foo/bar`  -> anchored (contains a slash)
/// * `*.pem`    -> no slash, matches at any depth (`**/*.pem`)
/// * `dir/`     -> everything under dir (`dir/**`)
pub fn normalize_path_pattern(p: &str) -> String {
    let mut s = p.trim().trim_start_matches("./").to_string();
    if s.ends_with('/') {
        s.push_str("**");
    }
    if let Some(rest) = s.strip_prefix('/') {
        return rest.to_string();
    }
    if !s.contains('/') {
        return format!("**/{s}");
    }
    s
}

pub fn path_set(patterns: &[String], case_insensitive: bool) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let norm = normalize_path_pattern(p);
        let g = GlobBuilder::new(&norm)
            .literal_separator(true)
            .case_insensitive(case_insensitive)
            .backslash_escape(true)
            .build()
            .with_context(|| format!("invalid path glob `{p}`"))?;
        b.add(g);
    }
    Ok(b.build()?)
}

/// Globs for symbol names; `*` may cross `::`/`.` separators.
pub fn name_set(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let g = GlobBuilder::new(p.trim())
            .literal_separator(false)
            .backslash_escape(true)
            .build()
            .with_context(|| format!("invalid symbol glob `{p}`"))?;
        b.add(g);
    }
    Ok(b.build()?)
}

/// Escape a literal path so it can be used as an anchored glob.
pub fn escape_literal(path: &str) -> String {
    let mut out = String::from("/");
    for c in path.chars() {
        match c {
            '*' | '?' | '[' | ']' | '{' | '}' | '\\' => {
                out.push('[');
                out.push(c);
                out.push(']');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Lexically normalize a relative path (resolve `.` and `..`). Returns None if it
/// escapes the repository root.
pub fn normalize_rel(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_like_semantics() {
        let s = path_set(
            &["*.pem".into(), "/.github/workflows/**".into(), "secrets/".into()],
            true,
        )
        .unwrap();
        assert!(s.is_match("a/b/key.pem"));
        assert!(s.is_match("KEY.PEM"));
        assert!(s.is_match(".github/workflows/ci.yml"));
        assert!(!s.is_match("x/.github/workflows/ci.yml"));
        assert!(s.is_match("secrets/a/b"));
        let star = path_set(&["src/*.rs".into()], false).unwrap();
        assert!(star.is_match("src/main.rs"));
        assert!(!star.is_match("src/a/main.rs"));
    }

    #[test]
    fn literal_escape() {
        let s = path_set(&[escape_literal("weird[1].toml")], false).unwrap();
        assert!(s.is_match("weird[1].toml"));
        assert!(!s.is_match("weird1.toml"));
    }

    #[test]
    fn rel_normalization() {
        assert_eq!(normalize_rel("a/./b/../c").as_deref(), Some("a/c"));
        assert_eq!(normalize_rel("../x"), None);
    }
}
