//! `gwt cd` — move into a worktree.
//!
//! With no argument this goes to the main worktree, mirroring how a bare `cd`
//! takes you home. Choosing from a list is `gwt list`.

use anyhow::Result;

use crate::cd_target;
use crate::cli::CdArgs;
use crate::repo::Repo;

pub fn run(args: CdArgs) -> Result<()> {
    let repo = Repo::discover()?;
    let worktree = repo.resolve(args.name.as_deref())?;
    cd_target::request(&worktree.path)
}
