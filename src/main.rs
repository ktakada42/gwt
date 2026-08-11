mod cli;
mod commands;
mod config;
mod git;
mod hooks;
mod repo;

use clap::Parser;

use cli::{Cli, Command};

fn main() {
    if let Err(err) = run() {
        eprintln!("gwt: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Add(args) => commands::add::run(args),
        Command::List(args) => commands::list::run(args),
        Command::Cd(args) => commands::cd::run(args),
        Command::Remove(args) => commands::remove::run(args),
        Command::Init(args) => commands::init::run(args),
        Command::ShellInit(args) => commands::shell::shell_init(args),
        Command::Completion(args) => commands::shell::completion(args),
    }
}
