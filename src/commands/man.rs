//! `gwx man` — write man pages, for whoever packages gwx.

use anyhow::{Context, Result};
use clap::CommandFactory;

use crate::cli::{Cli, ManArgs};

pub fn run(args: ManArgs) -> Result<()> {
    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("failed to create {}", args.out_dir.display()))?;

    // Writes gwx.1 plus a page per subcommand, so `man gwx-add` works too.
    clap_mangen::generate_to(Cli::command(), &args.out_dir)
        .with_context(|| format!("failed to write man pages to {}", args.out_dir.display()))?;

    eprintln!("Wrote man pages to {}", args.out_dir.display());
    Ok(())
}
