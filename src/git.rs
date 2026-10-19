//! Thin, explicit wrapper around the `git` CLI.
//!
//! Everything that reads content goes through blob ids (`git cat-file --batch`), and
//! diffs are taken with `--raw -z --no-renames --no-ext-diff --no-textconv`, so
//! repository-controlled settings (diff drivers, textconv, rename detection) can't
//! change what twokey sees.

use anyhow::{Context, Result, anyhow, bail};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
pub const MODE_SYMLINK: u32 = 0o120000;
pub const MODE_SUBMODULE: u32 = 0o160000;
pub const MODE_EXEC: u32 = 0o100755;

pub struct Repo {
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub old_mode: u32,
    pub new_mode: u32,
    pub old_sha: String,
    pub new_sha: String,
    pub status: char,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Added,
    Modified,
    Deleted,
}

/// One file's before/after state. `old`/`new` are `None` when that side doesn't
/// exist (or is a submodule, whose content isn't a blob).
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub status: Status,
    pub old_mode: u32,
    pub new_mode: u32,
    pub old: Option<Vec<u8>>,
    pub new: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub committer: String,
    pub sig_status: String,
    pub sig_key: String,
    pub subject: String,
    pub body: String,
}

impl Repo {
    pub fn discover(dir: &Path) -> Result<Repo> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "--show-toplevel"])
            .stderr(Stdio::piped())
            .output()
            .context("failed to run git (is it installed?)")?;
        if !out.status.success() {
            bail!("{} is not inside a git repository", dir.display());
        }
        let root = String::from_utf8(out.stdout)?.trim().to_string();
        let root = PathBuf::from(root);
        let root = root.canonicalize().unwrap_or(root);
        Ok(Repo { root })
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(&self.root)
            // Never let repo config page, colorize or run external diff tools.
            .args([
                "-c",
                "core.pager=cat",
                "-c",
                "color.ui=false",
                "-c",
                "core.quotepath=off",
            ])
            .args(args)
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null());
        c
    }

    pub fn git(&self, args: &[&str]) -> Result<Vec<u8>> {
        let out = self
            .cmd(args)
            .output()
            .with_context(|| format!("failed to run git {}", args.join(" ")))?;
        if !out.status.success() {
            bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(out.stdout)
    }

    pub fn git_opt(&self, args: &[&str]) -> Option<Vec<u8>> {
        let out = self.cmd(args).stderr(Stdio::null()).output().ok()?;
        out.status.success().then_some(out.stdout)
    }

    pub fn git_str(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.git(args)?).trim().to_string())
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        let spec = format!("{rev}^{{commit}}");
        self.git_str(&["rev-parse", "--verify", "--quiet", &spec])
            .map_err(|_| anyhow!("cannot resolve revision `{rev}`"))
    }

    pub fn try_rev_parse(&self, rev: &str) -> Option<String> {
        self.rev_parse(rev).ok()
    }

    /// Absolute path of the git directory (handles worktrees and `.git` files).
    pub fn git_dir(&self) -> Result<PathBuf> {
        let d = PathBuf::from(self.git_str(&["rev-parse", "--absolute-git-dir"])?);
        Ok(d)
    }

    pub fn has_head(&self) -> bool {
        self.try_rev_parse("HEAD").is_some()
    }

    pub fn merge_base(&self, a: &str, b: &str) -> Result<String> {
        self.git_str(&["merge-base", a, b])
            .map_err(|_| anyhow!("no merge base between `{a}` and `{b}` (shallow clone? use fetch-depth: 0)"))
    }

    pub fn current_branch(&self) -> Option<String> {
        let b = self.git_opt(&["symbolic-ref", "--short", "-q", "HEAD"])?;
        let s = String::from_utf8_lossy(&b).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    /// Content of `path` at `rev`, or None if it doesn't exist there.
    pub fn show_file(&self, rev: &str, path: &str) -> Option<Vec<u8>> {
        let spec = format!("{rev}:{path}");
        self.git_opt(&["cat-file", "blob", &spec])
    }

    pub fn diff_raw(&self, extra: &[&str]) -> Result<Vec<RawEntry>> {
        let mut args = vec![
            "diff",
            "--raw",
            "-z",
            "--no-renames",
            "--no-abbrev",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=none",
        ];
        args.extend_from_slice(extra);
        parse_raw(&self.git(&args)?)
    }

    pub fn untracked(&self) -> Result<Vec<String>> {
        let out = self.git(&["ls-files", "--others", "--exclude-standard", "-z"])?;
        Ok(split_nul(&out))
    }

    /// (mode, sha, path) for every entry in `rev`'s tree.
    pub fn ls_tree(&self, rev: &str) -> Result<Vec<(u32, String, String)>> {
        let out = self.git(&["ls-tree", "-r", "-z", "--full-tree", rev])?;
        let mut v = Vec::new();
        for rec in split_nul(&out) {
            let (meta, path) = rec.split_once('\t').ok_or_else(|| anyhow!("bad ls-tree line"))?;
            let mut it = meta.split(' ');
            let mode = u32::from_str_radix(it.next().unwrap_or("0"), 8).unwrap_or(0);
            let _ty = it.next();
            let sha = it.next().unwrap_or("").to_string();
            v.push((mode, sha, path.to_string()));
        }
        Ok(v)
    }

    pub fn commits(&self, base: &str, head: &str) -> Result<Vec<CommitInfo>> {
        let range = format!("{base}..{head}");
        let list = self.git_str(&["rev-list", "--reverse", &range])?;
        let mut out = Vec::new();
        for sha in list.lines().filter(|l| !l.is_empty()) {
            let raw = self.git(&[
                "show",
                "-s",
                "--format=%P%x00%an <%ae>%x00%cn <%ce>%x00%G?%x00%GF%x00%s%x00%B",
                sha,
            ])?;
            let s = String::from_utf8_lossy(&raw).to_string();
            let p: Vec<&str> = s.splitn(7, '\0').collect();
            if p.len() < 7 {
                bail!("unexpected git show output for {sha}");
            }
            out.push(CommitInfo {
                sha: sha.to_string(),
                parents: p[0].split_whitespace().map(String::from).collect(),
                author: p[1].to_string(),
                committer: p[2].to_string(),
                sig_status: p[3].trim().to_string(),
                sig_key: p[4].trim().to_string(),
                subject: p[5].to_string(),
                body: p[6].to_string(),
            });
        }
        Ok(out)
    }

    /// Paths a (non-merge) commit touched relative to its first parent.
    pub fn commit_paths(&self, sha: &str) -> Result<BTreeSet<String>> {
        let out = self.git(&[
            "diff-tree",
            "-r",
            "-z",
            "--root",
            "--no-commit-id",
            "--name-only",
            "--no-renames",
            "--no-ext-diff",
            sha,
        ])?;
        Ok(split_nul(&out).into_iter().collect())
    }

    /// Paths a merge commit changed beyond what any parent had (conflict
    /// resolutions and "evil merge" edits).
    pub fn merge_paths(&self, sha: &str) -> Result<BTreeSet<String>> {
        let out = self.git(&["diff-tree", "-r", "-z", "--no-commit-id", "--name-only", "--cc", sha])?;
        Ok(split_nul(&out).into_iter().collect())
    }

    /// Turn raw diff entries into full before/after file states. When
    /// `worktree` is set, a zero sha on the new side means "read from disk".
    pub fn load_changes(&self, entries: &[RawEntry], worktree: bool) -> Result<Vec<FileChange>> {
        let mut out = Vec::with_capacity(entries.len());
        let mut reader = BlobReader::new(self)?;
        for e in entries {
            let status = match e.status {
                'A' => Status::Added,
                'D' => Status::Deleted,
                _ => Status::Modified,
            };
            let old = self.side(&mut reader, e.old_mode, &e.old_sha, &e.path, false)?;
            let new = self.side(&mut reader, e.new_mode, &e.new_sha, &e.path, worktree)?;
            out.push(FileChange {
                path: e.path.clone(),
                status,
                old_mode: e.old_mode,
                new_mode: e.new_mode,
                old,
                new,
            });
        }
        Ok(out)
    }

    fn side(
        &self,
        reader: &mut BlobReader,
        mode: u32,
        sha: &str,
        path: &str,
        worktree: bool,
    ) -> Result<Option<Vec<u8>>> {
        if mode == 0 || mode == MODE_SUBMODULE {
            return Ok(None);
        }
        if is_zero_sha(sha) {
            if !worktree {
                return Ok(None);
            }
            return self.read_disk(path, mode).map(Some);
        }
        reader.read(sha).map(Some)
    }

    pub fn read_disk(&self, rel: &str, mode: u32) -> Result<Vec<u8>> {
        let p = self.root.join(rel);
        if mode == MODE_SYMLINK {
            let t = std::fs::read_link(&p).with_context(|| format!("reading link {}", p.display()))?;
            return Ok(t.to_string_lossy().into_owned().into_bytes());
        }
        std::fs::read(&p).with_context(|| format!("reading {}", p.display()))
    }

    /// Working-tree state of untracked (non-ignored) files as additions.
    pub fn untracked_changes(&self) -> Result<Vec<FileChange>> {
        let mut out = Vec::new();
        for path in self.untracked()? {
            let full = self.root.join(&path);
            let meta = match std::fs::symlink_metadata(&full) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let ft = meta.file_type();
            if !(ft.is_file() || ft.is_symlink() || ft.is_dir()) {
                continue; // FIFOs, sockets, devices: reading could block forever
            }
            if meta.is_dir() {
                // An embedded repository shows up as `dir/`; treat it like a
                // submodule addition rather than failing to read it.
                out.push(FileChange {
                    path: path.trim_end_matches('/').to_string(),
                    status: Status::Added,
                    old_mode: 0,
                    new_mode: MODE_SUBMODULE,
                    old: None,
                    new: None,
                });
                continue;
            }
            let mode = if meta.file_type().is_symlink() {
                MODE_SYMLINK
            } else if is_executable(&meta) {
                MODE_EXEC
            } else {
                0o100644
            };
            let new = self.read_disk(&path, mode)?;
            out.push(FileChange {
                path,
                status: Status::Added,
                old_mode: 0,
                new_mode: mode,
                old: None,
                new: Some(new),
            });
        }
        Ok(out)
    }
}

/// One long-lived `git cat-file --batch` process, instead of a process per blob.
pub struct BlobReader {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl BlobReader {
    pub fn new(repo: &Repo) -> Result<BlobReader> {
        let mut child = repo
            .cmd(&["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("starting git cat-file --batch")?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin for git cat-file"))?;
        let stdout = std::io::BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("no stdout for git cat-file"))?,
        );
        Ok(BlobReader { child, stdin, stdout })
    }

    /// Read a blob by object id. Errors (never silently empty) if it's missing
    /// or not a blob.
    pub fn read(&mut self, sha: &str) -> Result<Vec<u8>> {
        use std::io::{BufRead, Read, Write};
        if sha.is_empty() || sha.contains(char::is_whitespace) {
            bail!("invalid object id `{sha}`");
        }
        writeln!(self.stdin, "{sha}")?;
        self.stdin.flush()?;
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        // "<sha> <type> <size>\n" or "<sha> missing\n"
        let mut parts = header.split_whitespace();
        let _id = parts.next();
        let ty = parts.next().unwrap_or("");
        if ty != "blob" {
            bail!("object {sha} is not a readable blob (`{}`)", header.trim());
        }
        let size: usize = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("bad cat-file header `{}`", header.trim()))?;
        let mut buf = vec![0u8; size];
        self.stdout.read_exact(&mut buf)?;
        let mut nl = [0u8; 1];
        self.stdout.read_exact(&mut nl)?;
        Ok(buf)
    }
}

impl Drop for BlobReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
fn is_executable(m: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode() & 0o111 != 0
}
#[cfg(not(unix))]
fn is_executable(_m: &std::fs::Metadata) -> bool {
    false
}

fn is_zero_sha(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b == b'0')
}

fn split_nul(b: &[u8]) -> Vec<String> {
    b.split(|c| *c == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Parse `git diff --raw -z` output:
/// `:old_mode new_mode old_sha new_sha STATUS\0path\0[path2\0]`
pub fn parse_raw(out: &[u8]) -> Result<Vec<RawEntry>> {
    let toks: Vec<String> = out
        .split(|c| *c == 0)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    let mut i = 0;
    let mut v = Vec::new();
    while i < toks.len() {
        let t = &toks[i];
        if t.is_empty() {
            i += 1;
            continue;
        }
        let meta = t
            .strip_prefix(':')
            .ok_or_else(|| anyhow!("unexpected raw diff token `{t}`"))?;
        let f: Vec<&str> = meta.split(' ').collect();
        if f.len() < 5 {
            bail!("malformed raw diff header `{t}`");
        }
        let status = f[4].chars().next().unwrap_or('M');
        let two_paths = matches!(status, 'R' | 'C');
        let path_a = toks.get(i + 1).cloned().ok_or_else(|| anyhow!("missing path"))?;
        let path = if two_paths {
            toks.get(i + 2).cloned().ok_or_else(|| anyhow!("missing path"))?
        } else {
            path_a
        };
        v.push(RawEntry {
            old_mode: u32::from_str_radix(f[0], 8).unwrap_or(0),
            new_mode: u32::from_str_radix(f[1], 8).unwrap_or(0),
            old_sha: f[2].to_string(),
            new_sha: f[3].to_string(),
            status,
            path,
        });
        i += if two_paths { 3 } else { 2 };
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_diff() {
        let raw = b":100644 100644 aaaa bbbb M\0src/a.rs\0:000000 120000 0000 cccc A\0link\0:100644 000000 dddd 0000 D\0gone.txt\0";
        let v = parse_raw(raw).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].path, "src/a.rs");
        assert_eq!(v[0].status, 'M');
        assert_eq!(v[1].new_mode, MODE_SYMLINK);
        assert_eq!(v[2].status, 'D');
        assert_eq!(v[2].new_mode, 0);
    }
}
