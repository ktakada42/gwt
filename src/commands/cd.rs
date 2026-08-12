//! `gwt cd` — move into a worktree.
//!
//! With no argument and a terminal to draw on, this opens the picker; the
//! chosen directory is handed to the shell through [`crate::cd_target`].

use anyhow::Result;

use crate::cd_target;
use crate::cli::CdArgs;
use crate::repo::Repo;
use crate::tui::{self, Outcome};

pub fn run(args: CdArgs) -> Result<()> {
    let repo = Repo::discover()?;

    if args.name.is_none() && tui::should_pick(&repo)? {
        return match tui::pick(&repo)? {
            // Silence on cancel: the shell only moves when it gets a path.
            Outcome::Cancelled => Ok(()),
            Outcome::Selected(path) => cd_target::request(&path),
        };
    }

    let worktree = repo.resolve(args.name.as_deref())?;
    cd_target::request(&worktree.path)
}
