//! `gwt list` — show every worktree of the repository.
//!
//! In a terminal this is the interactive picker; `--plain` and `--paths` are
//! the ways to ask for text a script can read.

use anyhow::Result;

use crate::cd_target;
use crate::cli::ListArgs;
use crate::git::Worktree;
use crate::repo::Repo;
use crate::tui::{self, Outcome};

pub fn run(args: ListArgs) -> Result<()> {
    let repo = Repo::discover()?;

    if !args.paths && !args.plain && tui::should_pick(&repo)? {
        return match tui::pick(&repo)? {
            Outcome::Cancelled => Ok(()),
            Outcome::Selected(path) => cd_target::request_picked(&path),
        };
    }

    let worktrees = repo.worktrees()?;

    if args.paths {
        for wt in &worktrees {
            println!("{}", wt.path.display());
        }
        return Ok(());
    }

    let Some(main) = worktrees.first() else {
        return Ok(());
    };

    let rows: Vec<Row> = worktrees
        .iter()
        .map(|wt| Row {
            current: wt.path == repo.cwd || repo.cwd.starts_with(&wt.path),
            name: repo.display_name(wt, main),
            head: wt.short_head(),
            note: note(wt),
            path: wt.path.display().to_string(),
        })
        .collect();

    let name_width = rows
        .iter()
        .map(|r| r.name.chars().count())
        .max()
        .unwrap_or(0);

    for row in &rows {
        let marker = if row.current { '*' } else { ' ' };
        println!(
            "{marker} {:<name_width$}  {}  {}{}",
            row.name, row.head, row.path, row.note
        );
    }
    Ok(())
}

struct Row {
    current: bool,
    name: String,
    head: String,
    note: String,
    path: String,
}

fn note(wt: &Worktree) -> String {
    let mut notes = Vec::new();
    if wt.bare {
        notes.push("bare");
    }
    if wt.detached {
        notes.push("detached");
    }
    if wt.locked {
        notes.push("locked");
    }
    if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn notes_describe_worktree_state() {
        let mut wt = Worktree {
            path: PathBuf::from("/wt"),
            head: None,
            branch: None,
            bare: false,
            detached: true,
            locked: true,
        };
        assert_eq!(note(&wt), " (detached, locked)");
        wt.detached = false;
        wt.locked = false;
        assert_eq!(note(&wt), "");
    }
}
