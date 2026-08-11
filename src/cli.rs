//! Command line definition.

use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::{ArgValueCandidates, ArgValueCompleter, PathCompleter};

use crate::completion;

#[derive(Debug, Parser)]
#[command(
    name = "gwt",
    // Derived from `git describe` at build time; see build.rs.
    version = env!("GWT_VERSION"),
    about = "A friendly git worktree manager",
    long_about = "gwt creates, lists and navigates git worktrees, with automatic \
                  path layout and per-repository hooks configured in .gwt.toml.",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a worktree for a branch, creating the branch if needed
    Add(AddArgs),

    /// List the worktrees of this repository
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Print the path of a worktree (with shell integration, changes into it)
    Cd(CdArgs),

    /// Remove a worktree
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),

    /// Write a .gwt.toml template to the main worktree
    Init(InitArgs),

    /// Print shell integration (cd support and completions) for eval
    ShellInit(ShellArgs),

    /// Print a completion script only
    Completion(ShellArgs),
}

#[derive(Debug, clap::Args)]
pub struct AddArgs {
    /// Branch to check out. Created when it does not exist yet
    #[arg(add = ArgValueCandidates::new(completion::addable_branches))]
    pub branch: String,

    /// Start point for a newly created branch (commit, tag or branch)
    #[arg(
        long,
        value_name = "COMMIT-ISH",
        add = ArgValueCandidates::new(completion::start_points)
    )]
    pub from: Option<String>,

    /// Path of the worktree, overriding the configured base_dir layout
    #[arg(
        long,
        value_name = "PATH",
        add = ArgValueCompleter::new(PathCompleter::dir())
    )]
    pub path: Option<String>,

    /// Fail instead of creating a branch that does not exist
    #[arg(long)]
    pub no_create: bool,

    /// Skip pre_create and post_create hooks
    #[arg(long)]
    pub no_hooks: bool,

    /// Check out the branch even if another worktree already has it
    #[arg(long)]
    pub force: bool,

    /// Print only the worktree path
    #[arg(short, long)]
    pub quiet: bool,
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Print only worktree paths, one per line
    #[arg(long)]
    pub paths: bool,
}

#[derive(Debug, clap::Args)]
pub struct CdArgs {
    /// Worktree or branch name. Defaults to the main worktree
    #[arg(add = ArgValueCandidates::new(completion::worktrees))]
    pub name: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct RemoveArgs {
    /// Worktree or branch name
    #[arg(add = ArgValueCandidates::new(completion::removable_worktrees))]
    pub name: String,

    /// Remove the branch as well when it is merged into the main worktree
    #[arg(long)]
    pub with_branch: bool,

    /// Remove even when the worktree has uncommitted changes
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Overwrite an existing .gwt.toml
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, clap::Args)]
pub struct ShellArgs {
    /// Shell to generate the script for
    #[arg(value_enum)]
    pub shell: Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// The shell's adapter for environment-activated completions.
    pub fn completer(self) -> &'static dyn clap_complete::env::EnvCompleter {
        match self {
            Shell::Bash => &clap_complete::env::Bash,
            Shell::Zsh => &clap_complete::env::Zsh,
            Shell::Fish => &clap_complete::env::Fish,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn add_parses_flags() {
        let cli = Cli::parse_from(["gwt", "add", "feat/x", "--from", "main", "--quiet"]);
        match cli.command {
            Command::Add(args) => {
                assert_eq!(args.branch, "feat/x");
                assert_eq!(args.from.as_deref(), Some("main"));
                assert!(args.quiet);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn list_alias_works() {
        assert!(matches!(
            Cli::parse_from(["gwt", "ls"]).command,
            Command::List(_)
        ));
        assert!(matches!(
            Cli::parse_from(["gwt", "rm", "feat/x"]).command,
            Command::Remove(_)
        ));
    }

    #[test]
    fn cd_without_name_targets_the_main_worktree() {
        match Cli::parse_from(["gwt", "cd"]).command {
            Command::Cd(args) => assert!(args.name.is_none()),
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
