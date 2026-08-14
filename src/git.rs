//! Thin wrappers around the `git` CLI.

use std::collections::BTreeMap;
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

/// Environment variables that pin git to a particular repository or index.
///
/// gwx always means "the repository containing this directory", so it works
/// that out from the path it was handed, not from whatever a caller exported.
/// The case that matters is being run from a git hook: git gives its hooks a
/// `GIT_DIR`, and git reads that before it looks at the working directory, so
/// without this every call would land on the hook's repository instead.
///
/// Deliberately absent are `GIT_CONFIG_GLOBAL` and its relatives. Which
/// repository to act on is gwx's business; how git is configured is the
/// user's.
pub const REPO_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_PREFIX",
];

/// A `git` invocation rooted at `dir`, deaf to any ambient repository.
fn git_in(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    for var in REPO_ENV {
        cmd.env_remove(var);
    }
    cmd
}

/// Runs git in `dir` and returns trimmed stdout, failing on a non-zero exit.
pub fn output<I, S>(dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = git_in(dir)
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
/// gwx's own stdout is reserved for paths, so that `gwx add --quiet` and
/// `gwx cd` stay usable in command substitution.
pub fn run<I, S>(dir: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = git_in(dir)
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
    git_in(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The main worktree of the repository containing `cwd`.
///
/// Every path in the configuration is resolved against it, so that `gwx`
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

/// All local branch names.
pub fn local_branches(cwd: &Path) -> Result<Vec<String>> {
    let out = output(
        cwd,
        ["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    Ok(non_empty_lines(&out))
}

/// Remote-tracking branches as `(origin/feature/foo, feature/foo)`.
///
/// `refname:strip=3` drops `refs/remotes/<remote>/`, which keeps slashes in the
/// branch name intact. `<remote>/HEAD` is skipped: it is a symbolic ref, not a
/// branch anyone would want to check out.
pub fn remote_branches(cwd: &Path) -> Result<Vec<(String, String)>> {
    let out = output(
        cwd,
        [
            "for-each-ref",
            "--format=%(refname:short)%09%(refname:strip=3)",
            "refs/remotes",
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, short)| *short != "HEAD" && !short.is_empty())
        .map(|(full, short)| (full.to_string(), short.to_string()))
        .collect())
}

/// Everything that can serve as a start point for a new branch.
pub fn start_points(cwd: &Path) -> Result<Vec<String>> {
    let out = output(
        cwd,
        [
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/tags",
            "refs/remotes",
        ],
    )?;
    Ok(non_empty_lines(&out))
}

fn non_empty_lines(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// `true` if the worktree at `path` has staged or unstaged changes.
pub fn is_dirty(path: &Path) -> Result<bool> {
    Ok(!output(path, ["status", "--porcelain"])?.is_empty())
}

/// `true` if `branch` is fully contained in `HEAD` of the main worktree.
pub fn is_merged(main: &Path, branch: &str) -> Result<bool> {
    Ok(merged_branches(main)?.iter().any(|b| b == branch))
}

/// Every branch already contained in `HEAD` of the main worktree.
///
/// The picker asks about each worktree in turn; one call answers them all.
pub fn merged_branches(main: &Path) -> Result<Vec<String>> {
    let out = output(
        main,
        ["branch", "--merged", "HEAD", "--format=%(refname:short)"],
    )?;
    Ok(non_empty_lines(&out))
}

/// Where a branch stands against the remote branch it tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tracking {
    /// No upstream: the branch has never left this machine.
    Untracked,
    /// An upstream is configured, but it is no longer on the remote.
    Gone,
    /// Every commit is on the upstream.
    Pushed,
    /// This many commits are on the branch and not on its upstream.
    Ahead(usize),
}

/// How every local branch stands against its upstream.
///
/// One call for the whole repository, and the ahead count comes free with it:
/// asking `for-each-ref` for `%(upstream:track)` measured the same as asking
/// for the upstream name alone, and a tenth of the merged-branch check the
/// picker already makes.
pub fn tracking(main: &Path) -> Result<BTreeMap<String, Tracking>> {
    let out = output(
        main,
        [
            "for-each-ref",
            "--format=%(refname:short)\t%(upstream:short)\t%(upstream:track)",
            "refs/heads/",
        ],
    )?;

    let mut map = BTreeMap::new();
    for line in out.lines() {
        let mut fields = line.split('\t');
        let (Some(branch), Some(upstream)) = (fields.next(), fields.next()) else {
            continue;
        };
        if branch.is_empty() {
            continue;
        }
        map.insert(
            branch.to_string(),
            parse_tracking(upstream, fields.next().unwrap_or_default()),
        );
    }
    Ok(map)
}

/// Reads one row of `for-each-ref` output.
///
/// `%(upstream:track)` is empty both for a branch with no upstream and for one
/// that is level with it, so the upstream name is what tells those apart.
fn parse_tracking(upstream: &str, track: &str) -> Tracking {
    if upstream.is_empty() {
        return Tracking::Untracked;
    }
    if track.contains("gone") {
        return Tracking::Gone;
    }
    // "[ahead 3]", "[ahead 1, behind 2]", "[behind 2]", or empty. Behind on its
    // own is still fully pushed: the remote has everything this branch has.
    match track
        .trim_start_matches('[')
        .split(',')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("ahead "))
        .and_then(|n| n.trim_end_matches(']').parse().ok())
    {
        Some(ahead) => Tracking::Ahead(ahead),
        None => Tracking::Pushed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_tells_apart_never_pushed_and_level() {
        // The track field is empty in both cases; the upstream name is not.
        assert_eq!(parse_tracking("", ""), Tracking::Untracked);
        assert_eq!(parse_tracking("origin/feat", ""), Tracking::Pushed);
    }

    #[test]
    fn tracking_reads_the_ahead_count() {
        assert_eq!(
            parse_tracking("origin/feat", "[ahead 3]"),
            Tracking::Ahead(3)
        );
        assert_eq!(
            parse_tracking("origin/feat", "[ahead 1, behind 2]"),
            Tracking::Ahead(1)
        );
        // Behind alone means the remote has everything this branch has.
        assert_eq!(
            parse_tracking("origin/feat", "[behind 2]"),
            Tracking::Pushed
        );
        assert_eq!(parse_tracking("origin/feat", "[gone]"), Tracking::Gone);
    }
}
