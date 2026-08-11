//! Thin wrappers around the `git` CLI.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};

/// A single entry of `git worktree list --porcelain`.
#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    /// Short branch name (`refs/heads/` stripped), `None` when detached or bare.
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
}

impl Worktree {
    /// The name used to refer to this worktree on the command line.
    ///
    /// Branch name if it has one, otherwise the directory name.
    pub fn name(&self) -> String {
        self.branch.clone().unwrap_or_else(|| {
            self.path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.path.display().to_string())
        })
    }

    pub fn short_head(&self) -> String {
        match &self.head {
            Some(sha) => sha.chars().take(7).collect(),
            None => "-".to_string(),
        }
    }
}

/// Runs git in `dir` and returns trimmed stdout, failing on a non-zero exit.
pub fn output<I, S>(dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .context("failed to run `git` (is it installed and on PATH?)")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git failed: {}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// Runs git in `dir` for its side effects, failing on a non-zero exit.
///
/// Progress git prints on stdout ("HEAD is now at …") is forwarded to stderr:
/// gwt's own stdout is reserved for paths, so that `gwt add --quiet` and
/// `gwt cd` stay usable in command substitution.
pub fn run<I, S>(dir: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .context("failed to run `git` (is it installed and on PATH?)")?;

    let chatter = String::from_utf8_lossy(&out.stdout);
    if !chatter.trim().is_empty() {
        eprint!("{chatter}");
    }
    if !out.status.success() {
        bail!("git exited with status {}", out.status);
    }
    Ok(())
}

/// Returns true when git exits successfully, ignoring all output.
fn check<I, S>(dir: &Path, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The main worktree of the repository containing `cwd`.
///
/// Every path in the configuration is resolved against it, so that `gwt`
/// behaves the same no matter which worktree it is invoked from.
pub fn main_worktree(cwd: &Path) -> Result<PathBuf> {
    if !check(cwd, ["rev-parse", "--git-dir"]) {
        bail!("not inside a git repository");
    }
    // The first entry of `worktree list` is always the main worktree.
    let list = list_worktrees(cwd)?;
    list.into_iter()
        .next()
        .map(|w| w.path)
        .ok_or_else(|| anyhow!("could not determine the main worktree"))
}

pub fn list_worktrees(cwd: &Path) -> Result<Vec<Worktree>> {
    let out = output(cwd, ["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_list(&out))
}

pub fn parse_worktree_list(porcelain: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<Worktree> = None;

    for line in porcelain.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            worktrees.extend(current.take());
            continue;
        }
        let (key, value) = match line.split_once(' ') {
            Some((k, v)) => (k, v),
            None => (line, ""),
        };
        match key {
            "worktree" => {
                worktrees.extend(current.take());
                current = Some(Worktree {
                    path: PathBuf::from(value),
                    head: None,
                    branch: None,
                    bare: false,
                    detached: false,
                    locked: false,
                });
            }
            _ => {
                let Some(wt) = current.as_mut() else { continue };
                match key {
                    "HEAD" => wt.head = Some(value.to_string()),
                    "branch" => {
                        wt.branch = Some(
                            value
                                .strip_prefix("refs/heads/")
                                .unwrap_or(value)
                                .to_string(),
                        )
                    }
                    "bare" => wt.bare = true,
                    "detached" => wt.detached = true,
                    "locked" => wt.locked = true,
                    _ => {}
                }
            }
        }
    }
    worktrees.extend(current);
    worktrees
}

pub fn local_branch_exists(cwd: &Path, branch: &str) -> bool {
    check(
        cwd,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
}

/// Remote-tracking branches whose name after the remote matches `branch`.
///
/// Returns entries such as `origin/feature/foo`.
pub fn remote_branches_matching(cwd: &Path, branch: &str) -> Result<Vec<String>> {
    let out = output(
        cwd,
        [
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("refs/remotes/*/{branch}"),
        ],
    )?;
    Ok(out
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// `true` if the worktree at `path` has staged or unstaged changes.
pub fn is_dirty(path: &Path) -> Result<bool> {
    Ok(!output(path, ["status", "--porcelain"])?.is_empty())
}

/// `true` if `branch` is fully contained in `HEAD` of the main worktree.
pub fn is_merged(main: &Path, branch: &str) -> Result<bool> {
    let out = output(
        main,
        ["branch", "--merged", "HEAD", "--format=%(refname:short)"],
    )?;
    Ok(out.lines().any(|l| l.trim() == branch))
}
