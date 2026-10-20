//! Policy file (`.twokey.toml`) schema, loading and compilation.
//!
//! Unknown keys are errors and every glob / regex / tree-sitter query is compiled
//! up front, so a typo fails the check instead of silently disabling a rule.

use crate::git::Repo;
use crate::glob::{escape_literal, name_set, normalize_path_pattern, path_set};
use crate::lang::{ALL, Lang};
use anyhow::{Context, Result, anyhow, bail};
use globset::GlobSet;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use tree_sitter::Query;

pub const DEFAULT_POLICY_PATH: &str = ".twokey.toml";
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    #[default]
    Allow,
    Warn,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineSel {
    #[default]
    Added,
    Removed,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleKindCfg {
    Path,
    Symbol,
    Query,
    Content,
}

// ---------------------------------------------------------------- raw schema

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PolicyFile {
    pub version: Option<u32>,
    #[serde(default)]
    pub settings: SettingsCfg,
    #[serde(default)]
    pub ai: AiCfg,
    #[serde(default)]
    pub markers: MarkersCfg,
    #[serde(default)]
    pub limits: LimitsCfg,
    #[serde(default, rename = "rule")]
    pub rules: Vec<RuleCfg>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct SettingsCfg {
    /// Match path globs case-insensitively (defends against case games on
    /// case-insensitive filesystems). Default true.
    pub case_insensitive_paths: bool,
    /// Files bigger than this aren't parsed; if a syntax rule applies to one, the
    /// change is flagged as unverifiable instead (fail closed).
    pub max_parse_bytes: usize,
    /// Base used by the agent hook for `git push` checks.
    pub default_base: Option<String>,
    /// Extra paths that get the same protection as the policy file itself.
    pub protect: Vec<String>,
    /// `false` = report-only: findings are printed/annotated but `check` exits 0.
    /// Use it to tune a new policy before enforcing it.
    pub enforce: bool,
    /// Sub-policy files (globs), e.g. `["services/*/.twokey.toml"]`. Each may only
    /// add `[[rule]]`s, scoped to its own directory; it can't weaken anything
    /// here. Matching files are protected like this file.
    pub include: Vec<String>,
}

impl Default for SettingsCfg {
    fn default() -> Self {
        SettingsCfg {
            case_insensitive_paths: true,
            max_parse_bytes: 2_000_000,
            default_base: Some("origin/main".into()),
            protect: Vec::new(),
            enforce: true,
            include: Vec::new(),
        }
    }
}

/// A directory-scoped policy pulled in by `settings.include`. Rules only.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SubPolicyFile {
    pub version: Option<u32>,
    #[serde(default, rename = "rule")]
    pub rules: Vec<RuleCfg>,
}

/// Where `settings.include` files come from (a git revision, or the disk).
pub trait IncludeSource {
    /// Every file path in the tree (repo-relative, `/`-separated).
    fn files(&self) -> Result<Vec<String>>;
    fn read(&self, path: &str) -> Option<String>;
}

/// No includes available (tests, built-in policy).
pub struct NoIncludes;
impl IncludeSource for NoIncludes {
    fn files(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    fn read(&self, _: &str) -> Option<String> {
        None
    }
}

struct GitIncludes<'a> {
    repo: &'a Repo,
    rev: String,
}
impl IncludeSource for GitIncludes<'_> {
    fn files(&self) -> Result<Vec<String>> {
        Ok(self.repo.ls_tree(&self.rev)?.into_iter().map(|(_, _, p)| p).collect())
    }
    fn read(&self, path: &str) -> Option<String> {
        self.repo
            .show_file(&self.rev, path)
            .and_then(|b| String::from_utf8(b).ok())
    }
}

struct DiskIncludes<'a> {
    repo: &'a Repo,
}
impl IncludeSource for DiskIncludes<'_> {
    fn files(&self) -> Result<Vec<String>> {
        let out = self
            .repo
            .git(&["ls-files", "-z", "--cached", "--others", "--exclude-standard"])?;
        Ok(out
            .split(|c| *c == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect())
    }
    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.repo.root.join(path)).ok()
    }
}

/// Re-root a sub-policy's path pattern under its directory.
fn scope_pattern(dir: &str, p: &str) -> String {
    format!("{}/{}", escape_literal(dir), normalize_path_pattern(p))
}

/// Pull `settings.include` sub-policies into `raw.rules`, scoped and namespaced.
fn expand_includes(raw: &mut PolicyFile, policy_path: &str, src: &dyn IncludeSource) -> Result<Vec<String>> {
    let mut loaded = Vec::new();
    if raw.settings.include.is_empty() {
        return Ok(loaded);
    }
    let set = path_set(&raw.settings.include, raw.settings.case_insensitive_paths)?;
    let mut files: Vec<String> = src
        .files()?
        .into_iter()
        .filter(|f| set.is_match(f) && f != policy_path)
        .collect();
    files.sort();
    for f in files {
        let Some((dir, _)) = f.rsplit_once('/') else {
            bail!("included policy `{f}` must live in a subdirectory");
        };
        let text = src
            .read(&f)
            .ok_or_else(|| anyhow!("can't read included policy `{f}`"))?;
        let sub: SubPolicyFile = toml::from_str(&text).with_context(|| {
            format!("parsing included policy `{f}` (sub-policies may only contain [[rule]] tables)")
        })?;
        if let Some(v) = sub.version
            && v != 1
        {
            bail!("{f}: unsupported policy version {v}");
        }
        for mut r in sub.rules {
            if r.id.starts_with("twokey-") {
                bail!("{f}: rule `{}`: ids starting with `twokey-` are reserved", r.id);
            }
            r.id = format!("{dir}:{}", r.id);
            r.paths = if r.paths.is_empty() {
                vec![format!("{}/**", escape_literal(dir))]
            } else {
                r.paths.iter().map(|p| scope_pattern(dir, p)).collect()
            };
            r.exclude = r.exclude.iter().map(|p| scope_pattern(dir, p)).collect();
            raw.rules.push(r);
        }
        loaded.push(f);
    }
    Ok(loaded)
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct AiCfg {
    /// Treat changes as AI unless explicitly declared human.
    pub assume_ai: bool,
    /// Literal standalone declaration in the final commit-message paragraph.
    pub human_marker: String,
    /// Regexes matched against each line of each commit message.
    pub trailers: Option<Vec<String>>,
    /// Regexes matched against `Name <email>` of author and committer.
    pub authors: Vec<String>,
    /// Regexes matched against the source branch name.
    pub branches: Vec<String>,
    /// If any of these env vars is set (non-empty), local checks are AI.
    pub env: Vec<String>,
}

fn default_ai_trailers() -> Vec<String> {
    vec![
                r"(?i)^co-authored-by:.*\b(claude|anthropic|copilot|codex|openai|cursor|devin|gemini|aider|windsurf|jules|amazon q|codeium)\b".into(),
                r"(?i)^(ai-)?(assisted|generated)-by:".into(),
            ]
}

impl Default for AiCfg {
    fn default() -> Self {
        AiCfg {
            assume_ai: true,
            human_marker: "Human-Authored: true".into(),
            trailers: None,
            authors: vec![
                r"(?i)noreply@anthropic\.com".into(),
                r"(?i)\[bot\]".into(),
                r"(?i)\b(copilot|codex|devin|cursor)\b.*@".into(),
            ],
            branches: vec![r"^(claude|codex|copilot|cursor|devin|ai|agent|bot)[/-]".into()],
            env: vec!["CLAUDECODE".into(), "TWOKEY_AI".into()],
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct MarkersCfg {
    pub enabled: bool,
    pub lock_action: Action,
    pub human_action: Action,
}

impl Default for MarkersCfg {
    fn default() -> Self {
        MarkersCfg {
            enabled: true,
            lock_action: Action::Deny,
            human_action: Action::Allow,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct LimitsCfg {
    pub max_files: Option<usize>,
    pub max_changed_lines: Option<usize>,
    pub max_deleted_files: Option<usize>,
    /// Action when a size limit is exceeded.
    pub action: Action,
    pub symlinks: Action,
    pub submodules: Action,
    pub mode_changes: Action,
    pub binary: Action,
    /// Action for any limit finding on human-authored changes.
    pub human_action: Action,
}

impl Default for LimitsCfg {
    fn default() -> Self {
        LimitsCfg {
            max_files: None,
            max_changed_lines: None,
            max_deleted_files: None,
            action: Action::Deny,
            symlinks: Action::Deny,
            submodules: Action::Deny,
            mode_changes: Action::Deny,
            binary: Action::Warn,
            human_action: Action::Allow,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RuleCfg {
    pub id: String,
    pub kind: RuleKindCfg,
    #[serde(default = "default_action")]
    pub action: Action,
    #[serde(default)]
    pub human_action: Action,
    pub reason: Option<String>,
    /// Which files the rule covers. For `path` rules these are the protected paths.
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// `symbol`: name globs, matched against qualified (`auth::Session::verify`)
    /// and bare (`verify`) names.
    #[serde(default)]
    pub symbols: Vec<String>,
    /// `query`: grammar to compile the query for.
    pub language: Option<String>,
    /// `query`: a tree-sitter query; nodes captured as `@protect` are protected.
    pub query: Option<String>,
    pub capture: Option<String>,
    /// `content`: regex matched against changed lines.
    pub pattern: Option<String>,
    #[serde(default)]
    pub lines: LineSel,
    /// Which kinds of change trigger the rule. Default: all.
    #[serde(default)]
    pub on: Vec<ChangeKind>,
}

fn default_action() -> Action {
    Action::Deny
}

// ---------------------------------------------------------------- compiled

pub enum RuleSpec {
    Path,
    Symbol {
        names: GlobSet,
    },
    Query {
        queries: Vec<(Lang, Query, u32)>,
    },
    Content {
        re: Regex,
        lines: LineSel,
    },
    /// Built-in only: the file is protected if its base or head content matches.
    Mentions {
        re: Regex,
    },
}

/// Always protected, whatever the policy says: every twokey policy file
/// anywhere, twokey's config directories, and the trusted CI runner script.
pub const BUILTIN_PROTECTED: &[&str] = &[".twokey.toml", "/.twokey/**", "/.github/twokey/**"];

/// Places where twokey gets wired in. A file here is protected whenever it
/// mentions twokey (before or after the change).
pub const WIRING_LOCATIONS: &[&str] = &[
    "/.github/workflows/**",
    "/.github/actions/**",
    "/.gitlab-ci.yml",
    "/.gitlab/**",
    "/.circleci/**",
    "/.buildkite/**",
    "/Jenkinsfile",
    "/azure-pipelines.yml",
    "/bitbucket-pipelines.yml",
    "/.claude/**",
    "/.githooks/**",
    "/.husky/**",
    "/.pre-commit-config.yaml",
    "/.pre-commit-hooks.yaml",
    "/lefthook.yml",
    "/lefthook.yaml",
    "/.lefthook.yml",
    "/.lefthook.yaml",
];

pub struct Rule {
    pub id: String,
    pub kind: &'static str,
    pub action: Action,
    pub human_action: Action,
    pub reason: Option<String>,
    pub files: Option<GlobSet>,
    pub exclude: Option<GlobSet>,
    pub langs: Option<Vec<Lang>>,
    pub on: Vec<ChangeKind>,

    pub spec: RuleSpec,
}

impl Rule {
    pub fn covers(&self, path: &str) -> bool {
        if let Some(l) = &self.langs {
            match Lang::from_path(path) {
                Some(x) if l.contains(&x) => {}
                _ => return false,
            }
        }
        if let Some(f) = &self.files
            && !f.is_match(path)
        {
            return false;
        }
        if let Some(x) = &self.exclude
            && x.is_match(path)
        {
            return false;
        }
        true
    }

    pub fn triggers_on(&self, c: ChangeKind) -> bool {
        self.on.is_empty() || self.on.contains(&c)
    }
}

pub struct CompiledAi {
    pub cfg: AiCfg,
    pub trailers: Vec<Regex>,
    pub authors: Vec<Regex>,
    pub branches: Vec<Regex>,
}

pub struct Policy {
    pub source: String,
    pub settings: SettingsCfg,
    pub ai: CompiledAi,

    pub markers: MarkersCfg,
    pub limits: LimitsCfg,
    pub rules: Vec<Rule>,
    pub notes: Vec<String>,
}

fn regexes(v: &[String], what: &str) -> Result<Vec<Regex>> {
    v.iter()
        .map(|p| Regex::new(p).with_context(|| format!("invalid {what} regex `{p}`")))
        .collect()
}

impl Policy {
    #[cfg(test)]
    pub fn compile(text: &str, policy_path: &str, source: String) -> Result<Policy> {
        Self::compile_with(text, policy_path, source, &NoIncludes)
    }

    pub fn compile_with(text: &str, policy_path: &str, source: String, inc: &dyn IncludeSource) -> Result<Policy> {
        let mut raw: PolicyFile = toml::from_str(text).with_context(|| format!("parsing {policy_path}"))?;
        let loaded = expand_includes(&mut raw, policy_path, inc)?;
        let mut p = Self::from_raw(raw, policy_path, source)?;
        if !loaded.is_empty() {
            p.notes.push(format!("included sub-policies: {}", loaded.join(", ")));
        }
        Ok(p)
    }

    pub fn builtin(policy_path: &str, note: &str) -> Policy {
        let mut p = Self::compile_with("", policy_path, "built-in defaults".into(), &NoIncludes)
            .expect("built-in policy compiles");
        p.notes.push(note.to_string());
        p
    }

    fn from_raw(raw: PolicyFile, policy_path: &str, source: String) -> Result<Policy> {
        if let Some(v) = raw.version
            && v != 1
        {
            bail!("unsupported policy version {v} (this twokey understands version 1)");
        }
        let ci = raw.settings.case_insensitive_paths;
        let mut rules = Vec::new();
        let mut ids = HashSet::new();

        // Built-in, always on, can't be removed or weakened by any policy: twokey's
        // own configuration is never changeable by AI, human changes remain subject to repository protections. (A policy can only *add* paths via settings.protect.)
        let mut protected = vec![escape_literal(policy_path)];
        protected.extend(BUILTIN_PROTECTED.iter().map(|s| s.to_string()));
        protected.extend(raw.settings.protect.iter().cloned());
        protected.extend(raw.settings.include.iter().cloned());
        rules.push(Rule {
            id: "twokey-policy".into(),
            kind: "policy",
            action: Action::Deny,
            human_action: Action::Allow,
            reason: Some("twokey's own configuration must be changed by a human.".into()),
            files: Some(path_set(&protected, ci)?),
            exclude: None,
            langs: None,
            on: Vec::new(),

            spec: RuleSpec::Path,
        });
        // Built-in: CI, hook and agent config files that wire twokey in. Any such
        // file that mentions twokey before OR after the change is protected, so
        // removing the twokey step/hook is caught just like editing it.
        rules.push(Rule {
            id: "twokey-wiring".into(),
            kind: "policy",
            action: Action::Deny,
            human_action: Action::Allow,
            reason: Some(
                "This file runs or registers twokey (CI step, git hook, agent hook); only a human may change it."
                    .into(),
            ),
            files: Some(path_set(
                &WIRING_LOCATIONS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                ci,
            )?),
            exclude: None,
            langs: None,
            on: Vec::new(),

            spec: RuleSpec::Mentions {
                re: Regex::new(r"(?i)twokey").expect("static regex"),
            },
        });

        for r in raw.rules {
            let ctx = format!("rule `{}`", r.id);
            if r.id.trim().is_empty() {
                bail!("a rule is missing `id`");
            }
            if r.id.starts_with("twokey-") {
                bail!("{ctx}: ids starting with `twokey-` are reserved");
            }
            if !ids.insert(r.id.clone()) {
                bail!("duplicate rule id `{}`", r.id);
            }
            let files = if r.paths.is_empty() {
                None
            } else {
                Some(path_set(&r.paths, ci).context(ctx.clone())?)
            };
            let exclude = if r.exclude.is_empty() {
                None
            } else {
                Some(path_set(&r.exclude, ci).context(ctx.clone())?)
            };
            let unexpected = |field: bool, name: &str| -> Result<()> {
                if field {
                    bail!("{ctx}: `{name}` is not valid for kind = \"{:?}\"", r.kind);
                }
                Ok(())
            };
            let (kind, spec, langs) = match r.kind {
                RuleKindCfg::Path => {
                    if files.is_none() {
                        bail!("{ctx}: path rules need `paths`");
                    }
                    unexpected(!r.symbols.is_empty(), "symbols")?;
                    unexpected(r.query.is_some(), "query")?;
                    unexpected(r.pattern.is_some(), "pattern")?;
                    ("path", RuleSpec::Path, None)
                }
                RuleKindCfg::Symbol => {
                    if r.symbols.is_empty() {
                        bail!("{ctx}: symbol rules need `symbols` (use [\"*\"] for every symbol)");
                    }
                    unexpected(r.query.is_some(), "query")?;
                    unexpected(r.pattern.is_some(), "pattern")?;
                    let langs = match &r.language {
                        Some(l) => vec![Lang::from_name(l).ok_or_else(|| anyhow!("{ctx}: unknown language `{l}`"))?],
                        None => ALL.to_vec(),
                    };
                    (
                        "symbol",
                        RuleSpec::Symbol {
                            names: name_set(&r.symbols).context(ctx.clone())?,
                        },
                        Some(langs),
                    )
                }
                RuleKindCfg::Query => {
                    let src = r
                        .query
                        .as_deref()
                        .ok_or_else(|| anyhow!("{ctx}: query rules need `query`"))?;
                    let lname = r
                        .language
                        .as_deref()
                        .ok_or_else(|| anyhow!("{ctx}: query rules need `language`"))?;
                    let lang = Lang::from_name(lname).ok_or_else(|| anyhow!("{ctx}: unknown language `{lname}`"))?;
                    unexpected(r.pattern.is_some(), "pattern")?;
                    let q = Query::new(&lang.ts_language(), src)
                        .map_err(|e| anyhow!("{ctx}: invalid {} query: {e}", lang.name()))?;
                    let cap_name = r.capture.clone().unwrap_or_else(|| "protect".into());
                    let idx = q
                        .capture_index_for_name(&cap_name)
                        .ok_or_else(|| anyhow!("{ctx}: query has no @{cap_name} capture"))?;
                    (
                        "query",
                        RuleSpec::Query {
                            queries: vec![(lang, q, idx)],
                        },
                        Some(vec![lang]),
                    )
                }
                RuleKindCfg::Content => {
                    let p = r
                        .pattern
                        .as_deref()
                        .ok_or_else(|| anyhow!("{ctx}: content rules need `pattern`"))?;
                    unexpected(r.query.is_some(), "query")?;
                    let re = Regex::new(p).with_context(|| format!("{ctx}: invalid pattern"))?;
                    ("content", RuleSpec::Content { re, lines: r.lines }, None)
                }
            };
            rules.push(Rule {
                id: r.id,
                kind,
                action: r.action,
                human_action: r.human_action,
                reason: r.reason,
                files,
                exclude,
                langs,
                on: r.on,

                spec,
            });
        }

        if raw.ai.human_marker.trim().is_empty()
            || raw.ai.human_marker.trim() != raw.ai.human_marker
            || raw.ai.human_marker.chars().any(char::is_control)
        {
            bail!(
                "ai.human_marker must be a non-empty single line without surrounding whitespace or control characters"
            );
        }
        if !raw.ai.assume_ai && raw.ai.trailers.as_ref().is_none_or(Vec::is_empty) {
            bail!(
                "ai.assume_ai = false requires an explicit, non-empty ai.trailers list; define AI commit-message patterns or keep assume_ai = true"
            );
        }
        let trailer_patterns = raw.ai.trailers.clone().unwrap_or_else(default_ai_trailers);
        if trailer_patterns.iter().any(|p| p.trim().is_empty()) {
            bail!("ai.trailers must not contain blank patterns");
        }
        let ai = CompiledAi {
            trailers: regexes(&trailer_patterns, "ai.trailers")?,
            authors: regexes(&raw.ai.authors, "ai.authors")?,
            branches: regexes(&raw.ai.branches, "ai.branches")?,
            cfg: raw.ai,
        };

        Ok(Policy {
            source,
            settings: raw.settings,
            ai,

            markers: raw.markers,
            limits: raw.limits,
            rules,
            notes: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------- loading

#[derive(Debug, Clone, Default, clap::Args)]
pub struct PolicyArgs {
    /// Use this local policy file instead of reading it from git (testing only:
    /// in CI the policy must come from the base revision).
    #[arg(long, value_name = "FILE")]
    pub policy: Option<PathBuf>,
    /// Path of the policy file inside the repository.
    #[arg(long, default_value = DEFAULT_POLICY_PATH, value_name = "PATH")]
    pub policy_path: String,
    /// Revision to read the policy from (default: the base of the comparison).
    #[arg(long, value_name = "REV")]
    pub policy_ref: Option<String>,
    /// Fail if no policy file exists at the policy revision.
    #[arg(long)]
    pub require_policy: bool,
}

pub fn load(repo: &Repo, args: &PolicyArgs, default_ref: Option<&str>) -> Result<Policy> {
    let ppath = args.policy_path.trim_start_matches("./").to_string();
    if let Some(file) = &args.policy {
        let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        let mut p = Policy::compile_with(
            &text,
            &ppath,
            format!("{} (local file)", file.display()),
            &DiskIncludes { repo },
        )?;
        p.notes
            .push("policy read from a local file, not from git; don't do this in CI".into());
        return Ok(p);
    }
    let rev = args.policy_ref.as_deref().or(default_ref);
    if let Some(rev) = rev {
        let sha = repo.rev_parse(rev)?;
        if let Some(bytes) = repo.show_file(&sha, &ppath) {
            let text = String::from_utf8(bytes).with_context(|| format!("{ppath} is not UTF-8"))?;
            return Policy::compile_with(
                &text,
                &ppath,
                format!("{ppath} @ {} ({rev})", &sha[..sha.len().min(10)]),
                &GitIncludes { repo, rev: sha.clone() },
            );
        }
        if args.require_policy {
            bail!("no {ppath} at {rev}; refusing to run without a policy (--require-policy)");
        }
        return Ok(Policy::builtin(
            &ppath,
            &format!("no {ppath} at {rev}: only built-in protections apply"),
        ));
    }
    if args.require_policy {
        bail!("no revision to read {ppath} from (empty repository?)");
    }
    Ok(Policy::builtin(
        &ppath,
        "no commits yet: only built-in protections apply",
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_keys_and_bad_queries() {
        assert!(
            Policy::compile(
                "[[rule]]\nid='x'\nkind='path'\npaths=['a']\nactoin='deny'",
                ".twokey.toml",
                "t".into()
            )
            .is_err()
        );
        assert!(
            Policy::compile(
                "[[rule]]\nid='q'\nkind='query'\nlanguage='rust'\nquery='(no_such_node) @protect'",
                ".twokey.toml",
                "t".into()
            )
            .is_err()
        );
        assert!(
            Policy::compile(
                "[[rule]]\nid='q'\nkind='query'\nlanguage='rust'\nquery='(unsafe_block) @protect'",
                ".twokey.toml",
                "t".into()
            )
            .is_ok()
        );
    }

    #[test]
    fn example_policy_compiles() {
        let text = include_str!("../examples/twokey.toml");
        let p = Policy::compile(text, ".twokey.toml", "example".into()).unwrap();
        assert_eq!(p.rules.len(), 2, "starter enables only built-in rules");
        let enabled = enabled_example();
        let p = Policy::compile(&enabled, ".twokey.toml", "example".into()).unwrap();
        assert!(p.rules.len() > 5);
    }

    pub fn enabled_example() -> String {
        let mut in_rules = false;
        include_str!("../examples/twokey.toml")
            .lines()
            .map(|line| {
                in_rules |= line == "# [[rule]]";
                if in_rules {
                    line.strip_prefix("# ").unwrap_or(line)
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    struct FakeTree(Vec<(&'static str, &'static str)>);
    impl IncludeSource for FakeTree {
        fn files(&self) -> Result<Vec<String>> {
            Ok(self.0.iter().map(|(p, _)| p.to_string()).collect())
        }
        fn read(&self, path: &str) -> Option<String> {
            self.0.iter().find(|(p, _)| *p == path).map(|(_, t)| t.to_string())
        }
    }

    #[test]
    fn sub_policies_are_scoped_and_add_only() {
        let tree = FakeTree(vec![
            (
                "services/pay/.twokey.toml",
                "[[rule]]\nid='ledger'\nkind='path'\npaths=['/ledger/**', '*.sql']\naction='deny'\n",
            ),
            (
                "services/web/.twokey.toml",
                "[[rule]]\nid='all'\nkind='content'\npattern='TODO'\n",
            ),
        ]);
        let root = "[settings]\ninclude = ['services/*/.twokey.toml']\n";
        let p = Policy::compile_with(root, ".twokey.toml", "t".into(), &tree).unwrap();
        let ledger = p.rules.iter().find(|r| r.id == "services/pay:ledger").unwrap();
        assert!(ledger.covers("services/pay/ledger/a.rs"));
        assert!(ledger.covers("services/pay/x/y.sql"));
        assert!(!ledger.covers("ledger/a.rs"));
        assert!(!ledger.covers("services/web/z.sql"));
        let web = p.rules.iter().find(|r| r.id == "services/web:all").unwrap();
        assert!(web.covers("services/web/a.ts") && !web.covers("services/pay/a.ts"));
        // the sub-policy files themselves are protected like the root policy
        assert!(p.rules[0].covers("services/pay/.twokey.toml"));
        // sub-policies can't carry settings
        let bad = FakeTree(vec![("a/.twokey.toml", "[ai]\nassume_ai = false\n")]);
        assert!(
            Policy::compile_with(
                "[settings]\ninclude=['*/.twokey.toml']\n",
                ".twokey.toml",
                "t".into(),
                &bad
            )
            .is_err()
        );
    }

    #[test]
    fn signal_only_attribution_requires_explicit_trailers() {
        for suffix in ["", "\ntrailers=[]", "\ntrailers=['']", "\ntrailers=['   ']"] {
            let text = format!("[ai]\nassume_ai=false{suffix}");
            let error = Policy::compile(&text, ".twokey.toml", "test".into()).err().unwrap();
            assert!(error.to_string().contains("ai.trailers"), "{error}");
        }
        let p = Policy::compile(
            "[ai]\nassume_ai=false\ntrailers=['^Generated-by:']",
            ".twokey.toml",
            "test".into(),
        )
        .unwrap();
        assert_eq!(p.ai.trailers.len(), 1);
        assert!(p.ai.trailers[0].is_match("Generated-by: local-model"));
        // Default-AI policies still work without explicit patterns.
        assert!(Policy::compile("", ".twokey.toml", "test".into()).is_ok());
    }

    #[test]
    fn rejects_removed_approval_configuration() {
        for text in [
            "[approvals]\napprovers=['alice']",
            "[[rule]]\nid='x'\nkind='path'\npaths=['a']\naction='approve'",
            "[[rule]]\nid='x'\nkind='path'\npaths=['a']\napprovers=['alice']",
            "[markers]\napprovers=['alice']",
            "[markers]\nreview_action='warn'",
            "[limits]\napprovers=['alice']",
        ] {
            assert!(Policy::compile(text, ".twokey.toml", "test".into()).is_err(), "{text}");
        }
    }
}
