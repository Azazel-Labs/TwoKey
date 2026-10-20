//! Deciding which changes count as AI-authored.
//!
//! Untagged changes are AI by default. The configured human marker declares
//! fully human work; explicit AI signals take precedence. Declarations are not
//! proof of authorship.

use crate::config::CompiledAi;
use crate::git::{CommitInfo, Repo};
use anyhow::Result;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
pub struct CommitVerdict {
    pub sha: String,
    pub subject: String,
    pub ai: bool,
    pub why: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attribution {
    /// Every change is attributed to AI (forced, branch match, AI merge commit...).
    pub all_ai: bool,
    pub reasons: Vec<String>,
    pub commits: Vec<CommitVerdict>,
    #[serde(skip)]
    ai_paths: HashSet<String>,
}

impl Attribution {
    pub fn whole(ai: bool, reason: impl Into<String>) -> Attribution {
        Attribution {
            all_ai: ai,
            reasons: vec![reason.into()],
            commits: Vec::new(),
            ai_paths: HashSet::new(),
        }
    }

    pub fn is_ai(&self, path: &str) -> bool {
        self.all_ai || self.ai_paths.contains(path)
    }
}

/// Require an unambiguous declaration in the final trailer paragraph, after
/// the subject/body. Prose mentions and conflicting/duplicate values do not count.
pub fn human_declared(marker: &str, message: &str) -> bool {
    let normalized = message.replace("\r\n", "\n");
    let Some((_, footer)) = normalized.trim_end().rsplit_once("\n\n") else {
        return false;
    };
    let mut matches = 0;
    for line in footer.lines() {
        if line.trim().eq_ignore_ascii_case(marker) {
            matches += 1;
            continue;
        }
        let Some((key, _)) = line.split_once(':') else {
            return false;
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return false;
        }
        // Reject a conflicting value for the configured trailer key.
        if marker
            .split_once(':')
            .is_some_and(|(expected, _)| key.eq_ignore_ascii_case(expected))
        {
            return false;
        }
    }
    matches == 1
}

pub fn classify_commit(ai: &CompiledAi, c: &CommitInfo) -> (bool, Vec<String>) {
    let mut why = Vec::new();
    for line in c.body.lines() {
        let line = line.trim();
        if ai.trailers.iter().any(|re| re.is_match(line)) {
            why.push(format!("trailer `{line}`"));
        }
    }
    for (role, ident) in [("author", &c.author), ("committer", &c.committer)] {
        if ai.authors.iter().any(|re| re.is_match(ident)) {
            why.push(format!("{role} `{ident}`"));
        }
    }
    if ai.cfg.assume_ai && !human_declared(&ai.cfg.human_marker, &c.body) {
        why.push(format!("no `{}` declaration (ai.assume_ai)", ai.cfg.human_marker));
    }
    (!why.is_empty(), why)
}

pub fn branch_is_ai(ai: &CompiledAi, branch: Option<&str>) -> Option<String> {
    let b = branch?;
    ai.branches
        .iter()
        .any(|re| re.is_match(b))
        .then(|| format!("branch `{b}`"))
}

pub fn env_is_ai(ai: &CompiledAi) -> Option<String> {
    ai.cfg
        .env
        .iter()
        .find(|v| std::env::var(v).map(|x| !x.is_empty()).unwrap_or(false))
        .map(|v| format!("environment variable {v} is set"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Force {
    None,
    Ai,
    Human,
}

/// Attribution for a commit range (CI mode). AI-ness is tracked per path: a
/// finding is AI-attributed if any AI commit in the range touched that path.
pub fn for_range(
    repo: &Repo,
    ai: &CompiledAi,
    base: &str,
    head: &str,
    branch: Option<&str>,
    force: Force,
) -> Result<Attribution> {
    match force {
        Force::Ai => return Ok(Attribution::whole(true, "--ai flag")),
        Force::Human => return Ok(Attribution::whole(false, "--human flag")),
        Force::None => {}
    }
    let commits = repo.commits(base, head)?;
    let mut att = Attribution::whole(false, "commit analysis");
    att.reasons.clear();
    if let Some(r) = branch_is_ai(ai, branch) {
        att.all_ai = true;
        att.reasons.push(r);
    }
    let mut merges: Vec<String> = Vec::new();
    for c in &commits {
        let (is_ai, why) = classify_commit(ai, c);
        if is_ai {
            if c.parents.len() > 1 {
                att.all_ai = true;
                att.reasons.push(format!("AI-attributed merge commit {}", &c.sha[..10]));
            } else {
                att.ai_paths.extend(repo.commit_paths(&c.sha)?);
            }
        }
        if !is_ai && c.parents.len() > 1 {
            merges.push(c.sha.clone());
        }
        att.commits.push(CommitVerdict {
            sha: c.sha.clone(),
            subject: c.subject.clone(),
            ai: is_ai,
            why,
        });
    }
    // An agent that runs `git merge` leaves no trailer on the merge. If agents
    // worked on this range at all, conflict resolutions in merges count as AI.
    if att.commits.iter().any(|c| c.ai) {
        for m in merges {
            att.ai_paths.extend(repo.merge_paths(&m)?);
        }
    }
    Ok(att)
}

/// Attribution for local checks (staged / worktree / agent hook): there are no
/// commits yet, so the whole change is AI or not.
pub fn for_local(
    ai: &CompiledAi,
    branch: Option<&str>,
    force: Force,
    in_agent: bool,
    message: Option<&str>,
) -> Attribution {
    match force {
        Force::Ai => return Attribution::whole(true, "--ai flag"),
        Force::Human => return Attribution::whole(false, "--human flag"),
        Force::None => {}
    }
    if in_agent {
        return Attribution::whole(true, "running as an agent hook");
    }
    if let Some(r) = env_is_ai(ai) {
        return Attribution::whole(true, r);
    }
    if let Some(r) = branch_is_ai(ai, branch) {
        return Attribution::whole(true, r);
    }
    if let Some(message) = message {
        for line in message.lines() {
            if ai.trailers.iter().any(|re| re.is_match(line.trim())) {
                return Attribution::whole(true, format!("trailer `{}`", line.trim()));
            }
        }
    }
    if message.is_some_and(|m| human_declared(&ai.cfg.human_marker, m)) {
        return Attribution::whole(false, format!("`{}` declaration", ai.cfg.human_marker));
    }
    if ai.cfg.assume_ai {
        return Attribution::whole(true, format!("no `{}` declaration (ai.assume_ai)", ai.cfg.human_marker));
    }
    Attribution::whole(false, "no AI signals (env, branch) found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Policy;

    fn commit(author: &str, body: &str) -> CommitInfo {
        CommitInfo {
            sha: "a".repeat(40),
            parents: vec!["b".repeat(40)],
            author: author.into(),
            committer: author.into(),
            subject: body.lines().next().unwrap_or("").into(),
            body: body.into(),
        }
    }

    #[test]
    fn default_signals() {
        let p = Policy::compile("", ".twokey.toml", "t".into()).unwrap();
        assert!(
            classify_commit(
                &p.ai,
                &commit(
                    "Dev <dev@x.io>",
                    "fix\n\nCo-Authored-By: Claude <noreply@anthropic.com>"
                )
            )
            .0
        );
        assert!(classify_commit(&p.ai, &commit("dependabot[bot] <x@users.noreply.github.com>", "bump")).0);
        assert!(
            !classify_commit(
                &p.ai,
                &commit(
                    "Dev <dev@x.io>",
                    "fix\n\nCo-authored-by: Pat <pat@x.io>\nHuman-Authored: true"
                )
            )
            .0
        );
        assert!(branch_is_ai(&p.ai, Some("claude/fix-login")).is_some());
        assert!(branch_is_ai(&p.ai, Some("feature/claude-docs")).is_none());
    }

    #[test]
    fn human_declaration_requires_a_trailer() {
        for message in [
            "fix\n\nHuman-Authored: true",
            "fix\r\n\r\nhuman-authored: TRUE\r\n",
            "fix\n\nIssue: 123\nHuman-Authored: true\n",
        ] {
            assert!(human_declared("Human-Authored: true", message), "{message}");
        }
        for message in [
            "fix",
            "Human-Authored: true",
            "fix\nHuman-Authored: true",
            "fix\n\nHuman-Authored: false",
            "fix\n\nHuman-Authored: true\nprose",
            "fix\n\nHuman-Authored: true\n\nmore text",
            "fix\n\nHuman-Authored: true\nHuman-Authored: false",
            "fix\n\nHuman-Authored: true\nHuman-Authored: true",
        ] {
            assert!(!human_declared("Human-Authored: true", message), "{message}");
        }
    }

    #[test]
    fn default_ai_and_explicit_human_with_conflicting_signals() {
        let p = Policy::compile("", ".twokey.toml", "t".into()).unwrap();
        assert!(classify_commit(&p.ai, &commit("Dev <dev@x.io>", "fix")).0);
        let human = "fix\n\nHuman-Authored: true";
        assert!(!classify_commit(&p.ai, &commit("Dev <dev@x.io>", human)).0);
        assert!(classify_commit(&p.ai, &commit("bot[bot] <bot@x.io>", human)).0);
        assert!(
            classify_commit(
                &p.ai,
                &commit(
                    "Dev <dev@x.io>",
                    "fix\n\nGenerated-by: local-model\nHuman-Authored: true"
                )
            )
            .0
        );
        // Agent hooks never accept a commit-message declaration as permission.
        assert!(for_local(&p.ai, None, Force::None, true, Some(human)).all_ai);
        assert!(for_local(&p.ai, Some("ai/work"), Force::None, false, Some(human)).all_ai);
        let legacy = Policy::compile(
            "[ai]\nassume_ai=false\ntrailers=['^Generated-by:']",
            ".twokey.toml",
            "t".into(),
        )
        .unwrap();
        assert!(!classify_commit(&legacy.ai, &commit("Dev <dev@x.io>", "fix")).0);
    }

    #[test]
    fn custom_human_marker_replaces_default() {
        let p = Policy::compile("[ai]\nhuman_marker = '#Human'", ".twokey.toml", "t".into()).unwrap();
        for body in ["fix\n\n#Human", "fix\n\n#human\nIssue: 123"] {
            assert!(!classify_commit(&p.ai, &commit("Dev <dev@x.io>", body)).0);
        }
        for body in [
            "fix #Human",
            "fix\n\n#Human-ish",
            "fix\n\n#Human\n#Human",
            "fix\n\nHuman-Authored: true",
            "fix\n\nGenerated-by: model\n#Human",
        ] {
            assert!(classify_commit(&p.ai, &commit("Dev <dev@x.io>", body)).0);
        }
        for marker in ["", " ", " #Human", "#Human\nother"] {
            let toml = format!("[ai]\nhuman_marker = {marker:?}");
            assert!(Policy::compile(&toml, ".twokey.toml", "t".into()).is_err());
        }
    }
}
