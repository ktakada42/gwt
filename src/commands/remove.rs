//! `gwt remove` — delete a worktree, optionally with its branch.

use anyhow::{bail, Context, Result};

use crate::cli::RemoveArgs;
use crate::git::{self, Worktree};
use crate::repo::Repo;

/// What a removal is allowed to do.
#[derive(Debug, Clone, Copy, Default)]
pub struct RemoveOptions {
    /// Remove despite uncommitted changes, and delete an unmerged branch.
    pub force: bool,
    /// Delete the branch along with the worktree.
    pub with_branch: bool,
    /// Keep git quiet — the interactive picker owns the terminal.
    pub quiet: bool,
}

/// Refuses removals that would leave the user stranded or lose work.
///
/// Split out so the picker can show the same reasons before asking for
/// confirmation, rather than discovering them after the fact.
pub fn removal_blocker(repo: &Repo, worktree: &Worktree, force: bool) -> Option<String> {
    if worktree.path == repo.main {
        return Some("this is the main worktree".to_string());
    }
    if repo.cwd.starts_with(&worktree.path) {
        return Some("you are inside this worktree".to_string());
    }
    if !force && git::is_dirty(&worktree.path).unwrap_or(false) {
        return Some("it has uncommitted changes".to_string());
    }
    None
}

/// Removes a worktree, and its branch when asked.
pub fn remove_worktree(repo: &Repo, worktree: &Worktree, opts: RemoveOptions) -> Result<()> {
    if let Some(reason) = removal_blocker(repo, worktree, opts.force) {
        bail!("cannot remove {}: {reason}", worktree.path.display());
    }

    let mut git_args = vec!["worktree".to_string(), "remove".to_string()];
    if opts.force {
        git_args.push("--force".to_string());
    }
    git_args.push(worktree.path.display().to_string());
    run_git(&repo.main, git_args, opts.quiet)
        .with_context(|| format!("failed to remove {}", worktree.path.display()))?;

    if opts.with_branch {
        let Some(branch) = worktree.branch.as_deref() else {
            bail!("the worktree had no branch to remove");
        };
        if !opts.force && !git::is_merged(&repo.main, branch)? {
            bail!("branch `{branch}` is not merged into HEAD of the main worktree; pass --force to delete it anyway");
        }
        let flag = if opts.force { "-D" } else { "-d" };
        run_git(&repo.main, ["branch", flag, branch], opts.quiet)
            .with_context(|| format!("failed to delete branch `{branch}`"))?;
    }
    Ok(())
}

fn run_git<I, S>(dir: &std::path::Path, args: I, quiet: bool) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    if quiet {
        git::output(dir, args).map(|_| ())
    } else {
        git::run(dir, args)
    }
}

pub fn run(args: RemoveArgs) -> Result<()> {
    let repo = Repo::discover()?;
    let worktree = repo.resolve(Some(&args.name))?;

    // Keep the CLI's specific wording; the picker uses `removal_blocker`.
    if worktree.path == repo.main {
        bail!("refusing to remove the main worktree");
    }
    if repo.cwd.starts_with(&worktree.path) {
        bail!(
            "you are inside {}. Move out of it first (try `gwt cd @`)",
            worktree.path.display()
        );
    }
    if !args.force && git::is_dirty(&worktree.path).unwrap_or(false) {
        bail!(
            "{} has uncommitted changes. Commit them or pass --force",
            worktree.path.display()
        );
    }

    remove_worktree(
        &repo,
        &worktree,
        RemoveOptions {
            force: args.force,
            with_branch: args.with_branch,
            quiet: false,
        },
    )?;
    eprintln!("Removed {}", worktree.path.display());
    if args.with_branch {
        if let Some(branch) = worktree.branch.as_deref() {
            eprintln!("Deleted branch `{branch}`");
        }
    }
    Ok(())
}
