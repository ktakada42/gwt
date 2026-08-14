//! `gwx init` — drop a `.gwx.toml` template into the main worktree.

use anyhow::{bail, Context, Result};

use crate::cli::InitArgs;
use crate::config::{Config, TEMPLATE};
use crate::repo::Repo;

pub fn run(args: InitArgs) -> Result<()> {
    let repo = Repo::discover()?;
    let path = Config::path_in(&repo.main);

    if path.exists() && !args.force {
        bail!(
            "{} already exists (pass --force to overwrite)",
            path.display()
        );
    }
    std::fs::write(&path, TEMPLATE)
        .with_context(|| format!("failed to write {}", path.display()))?;
    eprintln!("Wrote {}", path.display());
    Ok(())
}
