//! `gwt remove` — delete a worktree, optionally with its branch.

use anyhow::{bail, Context, Result};

use crate::cli::RemoveArgs;
use crate::git;
use crate::repo::Repo;

pub fn run(args: RemoveArgs) -> Result<()> {
    let repo = Repo::discover()?;
    let worktree = repo.resolve(Some(&args.name))?;

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

    let mut git_args = vec!["worktree".to_string(), "remove".to_string()];
    if args.force {
        git_args.push("--force".to_string());
    }
    git_args.push(worktree.path.display().to_string());
    git::run(&repo.main, git_args)
        .with_context(|| format!("failed to remove {}", worktree.path.display()))?;
    eprintln!("Removed {}", worktree.path.display());

    if args.with_branch {
        let Some(branch) = worktree.branch.as_deref() else {
            bail!("the worktree had no branch to remove");
        };
        if !args.force && !git::is_merged(&repo.main, branch)? {
            bail!("branch `{branch}` is not merged into HEAD of the main worktree; pass --force to delete it anyway");
        }
        let flag = if args.force { "-D" } else { "-d" };
        git::run(&repo.main, ["branch", flag, branch])
            .with_context(|| format!("failed to delete branch `{branch}`"))?;
        eprintln!("Deleted branch `{branch}`");
    }

    Ok(())
}
