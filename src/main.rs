//! twokey — programmatic guardrails for AI-authored changes.
//!
//! Exit codes for `check`: 0 pass, 1 blocked (deny rule hit), 3 needs human
//! approval, 2 error (bad policy, git failure, ...). Errors never pass.

mod attribution;
mod config;
mod engine;
mod git;
mod glob;
mod hook;
mod lang;
mod markers;
mod report;
mod scan;
mod symbols;

use anyhow::{Context, Result, bail};
use attribution::Force;
use clap::{Args, Parser, Subcommand};
use config::PolicyArgs;
use git::Repo;
use report::Format;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "twokey",
    version,
    about = "Deny or require human approval for AI-authored changes to protected code"
)]
struct Cli {
    /// Repository to operate on (any directory inside it).
    #[arg(long, global = true, default_value = ".")]
    repo: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Evaluate a change (commit range, staged changes or working tree).
    Check(CheckArgs),
    /// Run as an agent hook (reads the hook payload on stdin).
    Hook {
        #[command(subcommand)]
        agent: HookAgent,
    },
    /// List everything the policy protects in a revision.
    Scan {
        #[command(flatten)]
        policy: PolicyArgs,
        /// Revision to scan.
        #[arg(long, default_value = "HEAD")]
        rev: String,
        #[arg(long)]
        json: bool,
    },
    /// Compile the policy (globs, regexes, tree-sitter queries) and report errors.
    Validate {
        #[command(flatten)]
        policy: PolicyArgs,
    },
    /// Print the GitHub teams (org/slug) the policy uses as approvers, one per
    /// line, so CI can fetch their membership into a --teams-file.
    Teams {
        #[command(flatten)]
        policy: PolicyArgs,
        /// Revision to read the policy from.
        #[arg(long, default_value = "HEAD")]
        rev: String,
    },
    /// Write an example .twokey.toml.
    Init {
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum HookAgent {
    /// Claude Code PreToolUse hook (Edit/Write/MultiEdit/NotebookEdit/Bash).
    Claude {
        #[command(flatten)]
        policy: PolicyArgs,
    },
}

#[derive(Args)]
struct CheckArgs {
    #[command(flatten)]
    policy: PolicyArgs,
    /// Base revision (CI). The comparison starts at merge-base(base, head).
    #[arg(long, conflicts_with_all = ["staged", "worktree"])]
    base: Option<String>,
    /// Head revision.
    #[arg(long, default_value = "HEAD")]
    head: String,
    /// Check staged changes (pre-commit).
    #[arg(long, conflicts_with = "worktree")]
    staged: bool,
    /// Check all uncommitted changes, including untracked files.
    #[arg(long)]
    worktree: bool,
    /// Logins of humans who approved this exact head (repeatable, or comma
    /// separated in TWOKEY_APPROVERS).
    #[arg(long = "approver", env = "TWOKEY_APPROVERS", value_delimiter = ',')]
    approvers: Vec<String>,
    /// Source branch name (used for ai.branches); defaults to the current branch.
    #[arg(long, env = "TWOKEY_BRANCH")]
    branch: Option<String>,
    /// Treat the whole change as AI-authored.
    #[arg(long, conflicts_with = "human")]
    ai: bool,
    /// Treat the whole change as human-authored.
    #[arg(long)]
    human: bool,
    /// JSON file mapping `org/slug` to member logins, for `team:` approvers
    /// (CI fetches it; see `twokey teams`).
    #[arg(long, env = "TWOKEY_TEAMS_FILE", value_name = "FILE")]
    teams_file: Option<PathBuf>,
    /// Report-only: print and annotate findings but always exit 0 (also
    /// `settings.enforce = false` in the policy).
    #[arg(long)]
    dry_run: bool,
    /// Also write a Markdown summary here (for a PR comment or job summary).
    #[arg(long, value_name = "FILE")]
    summary_out: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("twokey: error: {e:#}");
            2
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32> {
    match cli.cmd {
        Cmd::Check(a) => check(&cli.repo, a),
        Cmd::Hook {
            agent: HookAgent::Claude { policy },
        } => hook::run_claude(&cli.repo, &policy),
        Cmd::Scan { policy, rev, json } => {
            let repo = Repo::discover(&cli.repo)?;
            let p = config::load(&repo, &policy, Some(&rev))?;
            let items = scan::scan(&repo, &p, &rev)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                print!("{}", scan::render_text(&items));
            }
            Ok(0)
        }
        Cmd::Validate { policy } => {
            let repo = Repo::discover(&cli.repo)?;
            let head = repo.has_head().then_some("HEAD");
            let p = config::load(&repo, &policy, head)?;
            println!("ok: {} — {} rules (including built-ins)", p.source, p.rules.len());
            for n in &p.notes {
                println!("note: {n}");
            }
            Ok(0)
        }
        Cmd::Teams { policy, rev } => {
            let repo = Repo::discover(&cli.repo)?;
            let p = config::load(&repo, &policy, Some(&rev))?;
            for t in p.referenced_teams() {
                println!("{t}");
            }
            Ok(0)
        }
        Cmd::Init { force } => {
            let repo = Repo::discover(&cli.repo)?;
            let path = repo.root.join(config::DEFAULT_POLICY_PATH);
            if path.exists() && !force {
                bail!("{} already exists (use --force to overwrite)", path.display());
            }
            std::fs::write(&path, include_str!("../examples/twokey.toml"))?;
            println!("wrote {}", path.display());
            Ok(0)
        }
    }
}

fn check(repo_dir: &std::path::Path, a: CheckArgs) -> Result<i32> {
    let repo = Repo::discover(repo_dir)?;
    let _ = a.staged; // --staged is the default for local checks; the flag documents intent
    let force = if a.ai {
        Force::Ai
    } else if a.human {
        Force::Human
    } else {
        Force::None
    };
    let branch = a.branch.clone().or_else(|| repo.current_branch());
    let approvers: Vec<String> = a
        .approvers
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let (policy, changes, att) = if let Some(base) = &a.base {
        // CI: diff everything between merge-base and head.
        let head = repo.rev_parse(&a.head)?;
        let base_sha = repo.rev_parse(base)?;
        let mb = repo.merge_base(&base_sha, &head)?;
        // Policy from the tip of the target branch (not the merge-base): a branch
        // forked from an old commit must still meet today's rules.
        let policy = config::load(&repo, &a.policy, Some(&base_sha))?;
        let raw = repo.diff_raw(&[mb.as_str(), head.as_str()])?;
        let changes = repo.load_changes(&raw, false)?;
        let att = attribution::for_range(&repo, &policy.ai, &mb, &head, branch.as_deref(), force)?;
        (policy, changes, att)
    } else {
        let has_head = repo.has_head();
        let policy = config::load(&repo, &a.policy, has_head.then_some("HEAD"))?;
        let base = if has_head { "HEAD" } else { git::EMPTY_TREE };
        let changes = if a.worktree {
            hook::worktree_changes(&repo)?
        } else {
            // default and --staged: index vs HEAD (what the next commit contains)
            let raw = repo.diff_raw(&["--cached", base])?;
            repo.load_changes(&raw, false)?
        };
        let att = attribution::for_local(&policy.ai, branch.as_deref(), force, false);
        (policy, changes, att)
    };

    let mut policy = policy;
    if a.dry_run {
        policy.settings.enforce = false;
    }
    let mut approvals = engine::Approvals::from_logins(&approvers);
    if let Some(f) = &a.teams_file {
        let text = std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
        let parsed: std::collections::BTreeMap<String, Vec<String>> = serde_json::from_str(&text)
            .with_context(|| format!("{} must be a JSON object of \"org/slug\": [logins]", f.display()))?;
        approvals.teams = parsed.into_iter().map(|(k, v)| (k.trim().to_lowercase(), v)).collect();
    }
    let eval = engine::evaluate(&policy, &changes, &att, &approvals);
    print!("{}", report::render(&eval, a.format));
    if let Some(p) = &a.summary_out {
        std::fs::write(p, report::markdown(&eval)).with_context(|| format!("writing {}", p.display()))?;
    }
    Ok(eval.exit_code())
}
