//! Deciding which changes count as AI-authored.
//!
//! Heuristic signals (trailers, identities, branch names, env vars) are only as
//! honest as the agent. For a fail-closed setup use `require_signed_human = true`
//! (a commit is human only if it carries a good signature from a trusted key) or
//! `assume_ai = true`.

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
    if ai.cfg.require_signed_human {
        let good = c.sig_status == "G";
        let key_ok = ai.cfg.human_signing_keys.is_empty()
            || ai
                .cfg
                .human_signing_keys
                .iter()
                .any(|k| k.eq_ignore_ascii_case(&c.sig_key));
        if !(good && key_ok) {
            why.push(format!(
                "not signed by a trusted human key (signature status `{}`)",
                c.sig_status
            ));
        }
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
    if ai.cfg.assume_ai {
        att.all_ai = true;
        att.reasons.push("policy sets ai.assume_ai".into());
    }
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
pub fn for_local(ai: &CompiledAi, branch: Option<&str>, force: Force, in_agent: bool) -> Attribution {
    match force {
        Force::Ai => return Attribution::whole(true, "--ai flag"),
        Force::Human => return Attribution::whole(false, "--human flag"),
        Force::None => {}
    }
    if in_agent {
        return Attribution::whole(true, "running as an agent hook");
    }
    if ai.cfg.assume_ai {
        return Attribution::whole(true, "policy sets ai.assume_ai");
    }
    if let Some(r) = env_is_ai(ai) {
        return Attribution::whole(true, r);
    }
    if let Some(r) = branch_is_ai(ai, branch) {
        return Attribution::whole(true, r);
    }
    Attribution::whole(false, "no AI signals (env, branch) found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Policy;

    fn commit(author: &str, body: &str, sig: &str) -> CommitInfo {
        CommitInfo {
            sha: "a".repeat(40),
            parents: vec!["b".repeat(40)],
            author: author.into(),
            committer: author.into(),
            sig_status: sig.into(),
            sig_key: String::new(),
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
                    "fix\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
                    "N"
                )
            )
            .0
        );
        assert!(
            classify_commit(
                &p.ai,
                &commit("dependabot[bot] <x@users.noreply.github.com>", "bump", "N")
            )
            .0
        );
        assert!(
            !classify_commit(
                &p.ai,
                &commit("Dev <dev@x.io>", "fix\n\nCo-authored-by: Pat <pat@x.io>", "N")
            )
            .0
        );
        assert!(branch_is_ai(&p.ai, Some("claude/fix-login")).is_some());
        assert!(branch_is_ai(&p.ai, Some("feature/claude-docs")).is_none());
    }

    #[test]
    fn fail_closed_signatures() {
        let p = Policy::compile("[ai]\nrequire_signed_human = true\n", ".twokey.toml", "t".into()).unwrap();
        assert!(classify_commit(&p.ai, &commit("Dev <dev@x.io>", "fix", "N")).0);
        assert!(!classify_commit(&p.ai, &commit("Dev <dev@x.io>", "fix", "G")).0);
    }
}
