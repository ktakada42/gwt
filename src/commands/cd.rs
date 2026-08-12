//! `gwt cd` — print the path of a worktree.
//!
//! A process cannot change the directory of the shell that started it, so this
//! prints the path and the shell integration from `gwt shell-init` does the
//! `cd`. With no argument and a terminal to draw on, it opens the picker.

use anyhow::Result;

use crate::cli::CdArgs;
use crate::repo::Repo;
use crate::tui::{self, Outcome};

pub fn run(args: CdArgs) -> Result<()> {
    let repo = Repo::discover()?;

    if args.name.is_none() && should_pick(&repo)? {
        return match tui::pick(&repo)? {
            // Silence on cancel: the shell function only moves on output.
            Outcome::Cancelled => Ok(()),
            Outcome::Selected(path) => {
                println!("{}", path.display());
                Ok(())
            }
        };
    }

    let worktree = repo.resolve(args.name.as_deref())?;
    println!("{}", worktree.path.display());
    Ok(())
}

/// Whether a bare `gwt cd` should open the picker.
///
/// Without a terminal — a script, a pipeline, CI — it keeps the old behaviour
/// of resolving to the main worktree. A repository whose only worktree is the
/// main one has nothing to pick from either.
fn should_pick(repo: &Repo) -> Result<bool> {
    if !tui::is_available() {
        return Ok(false);
    }
    Ok(repo.worktrees()?.len() > 1)
}
