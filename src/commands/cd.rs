//! `gwt cd` — print the path of a worktree.
//!
//! A process cannot change its parent shell's directory, so this prints the
//! path and the shell integration from `gwt shell-init` does the `cd`.

use anyhow::Result;

use crate::cli::CdArgs;
use crate::repo::Repo;

pub fn run(args: CdArgs) -> Result<()> {
    let repo = Repo::discover()?;
    let worktree = repo.resolve(args.name.as_deref())?;
    println!("{}", worktree.path.display());
    Ok(())
}
