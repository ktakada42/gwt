//! `gwx clean` — review the worktrees that have outlived their branch and
//! remove the ones you pick.
//!
//! The judgement it makes rests on one fact: removing a worktree does not
//! remove commits. The branch keeps them, so the only thing a removal can
//! destroy is work that was never committed. Everything else the list reports
//! is about whether the work is *finished*, which is a different question and
//! the reason nothing but `done` is selected for you.

use anyhow::{bail, Result};

use crate::cli::CleanArgs;
use crate::commands::remove::{removal_blocker, remove_worktree, RemoveOptions};
use crate::git::{self, Tracking, Worktree};
use crate::repo::Repo;
use crate::tui;

/// What a worktree's state means for removing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Merged into the main worktree's HEAD, with nothing uncommitted.
    Done,
    /// Not merged, but every commit is on the upstream.
    Pushed,
    /// Commits that exist nowhere else, or no upstream at all.
    Local,
    /// Uncommitted changes, which a removal would destroy.
    Dirty,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Done => "done",
            State::Pushed => "pushed",
            State::Local => "local",
            State::Dirty => "dirty",
        }
    }

    /// Whether the row starts out ticked.
    ///
    /// Only `done`. The others are removable — `pushed` and `local` lose
    /// nothing, since the commits outlive the worktree — but they are live
    /// work, and a list that pre-selects live work gets used once.
    pub fn preselected(self) -> bool {
        self == State::Done
    }
}

/// A worktree offered for removal, with the reason for its state.
pub struct Candidate {
    pub worktree: Worktree,
    pub name: String,
    pub state: State,
    pub note: String,
}

pub fn run(args: CleanArgs) -> Result<()> {
    let repo = Repo::discover()?;
    let candidates = candidates(&repo)?;

    if candidates.is_empty() {
        eprintln!("Nothing to clean: no worktree besides the one you are in.");
        return Ok(());
    }

    // Without a terminal there is nobody to tick the boxes, and guessing is
    // the one thing a command that deletes things must not do.
    if !tui::is_available() {
        print_table(&candidates);
        eprintln!();
        eprintln!("gwx clean needs a terminal to choose in; nothing was removed.");
        return Ok(());
    }

    let Some(chosen) = tui::choose_to_clean(&candidates)? else {
        return Ok(());
    };
    if chosen.is_empty() {
        eprintln!("Nothing selected.");
        return Ok(());
    }

    let mut removed = 0;
    for index in chosen {
        let candidate = &candidates[index];
        if candidate.state == State::Dirty && !args.force {
            eprintln!(
                "Skipped {}: it has uncommitted changes (pass --force)",
                candidate.name
            );
            continue;
        }
        let opts = RemoveOptions {
            force: args.force,
            with_branch: args.with_branch,
            quiet: false,
            no_hooks: args.no_hooks,
        };
        match remove_worktree(&repo, &candidate.worktree, opts) {
            Ok(()) => {
                removed += 1;
                eprintln!("Removed {}", candidate.worktree.path.display());
            }
            // One failure should not strand the rest of the selection: the
            // user asked for several removals, not for a transaction.
            Err(e) => eprintln!("Failed to remove {}: {e:#}", candidate.name),
        }
    }
    eprintln!("Removed {removed} worktree(s).");
    Ok(())
}

/// Every worktree that could be removed, with its state worked out.
pub fn candidates(repo: &Repo) -> Result<Vec<Candidate>> {
    let worktrees = repo.worktrees()?;
    let Some(main) = worktrees.first().cloned() else {
        bail!("no worktrees found");
    };

    // Two calls for the whole repository rather than two per worktree.
    let merged = git::merged_branches(&repo.main)?;
    let tracking = git::tracking(&repo.main)?;

    let mut out = Vec::new();
    for worktree in worktrees.into_iter().skip(1) {
        if removal_blocker(repo, &worktree, true).is_some() {
            continue;
        }
        let dirty = git::is_dirty(&worktree.path).unwrap_or(false);
        let branch = worktree.branch.clone();
        let is_merged = branch.as_ref().is_some_and(|b| merged.contains(b));
        let track = branch
            .as_ref()
            .and_then(|b| tracking.get(b).copied())
            .unwrap_or(Tracking::Untracked);

        let (state, note) = classify(dirty, is_merged, track);
        out.push(Candidate {
            name: repo.display_name(&worktree, &main),
            worktree,
            state,
            note,
        });
    }
    Ok(out)
}

/// The state a worktree is in, and the sentence that explains it.
fn classify(dirty: bool, merged: bool, track: Tracking) -> (State, String) {
    if dirty {
        return (
            State::Dirty,
            "uncommitted changes would be lost".to_string(),
        );
    }
    if merged {
        return (
            State::Done,
            "merged into HEAD, nothing uncommitted".to_string(),
        );
    }
    match track {
        Tracking::Pushed => (
            State::Pushed,
            "not merged; every commit is on its upstream".to_string(),
        ),
        Tracking::Ahead(n) => (State::Local, format!("{n} commit(s) not on its upstream")),
        Tracking::Gone => (
            State::Local,
            "its upstream is gone from the remote".to_string(),
        ),
        Tracking::Untracked => (State::Local, "never pushed; it has no upstream".to_string()),
    }
}

/// The same rows as the picker, for a terminal that cannot draw one.
fn print_table(candidates: &[Candidate]) {
    let width = candidates
        .iter()
        .map(|c| c.name.chars().count())
        .max()
        .unwrap_or(0);
    for candidate in candidates {
        println!(
            "{:<width$}  {:<6}  {}",
            candidate.name,
            candidate.state.label(),
            candidate.note,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncommitted_changes_outrank_everything() {
        // Merged and pushed, but the edits in the working tree are the only
        // thing here that a removal could destroy.
        let (state, note) = classify(true, true, Tracking::Pushed);
        assert_eq!(state, State::Dirty);
        assert!(note.contains("uncommitted"));
    }

    #[test]
    fn only_merged_and_clean_is_preselected() {
        assert!(classify(false, true, Tracking::Untracked).0.preselected());
        for track in [Tracking::Pushed, Tracking::Ahead(2), Tracking::Gone] {
            assert!(!classify(false, false, track).0.preselected());
        }
        assert!(!classify(true, true, Tracking::Pushed).0.preselected());
    }

    #[test]
    fn an_unmerged_branch_is_told_apart_by_its_upstream() {
        assert_eq!(
            classify(false, false, Tracking::Pushed).0,
            State::Pushed,
            "everything is on the remote"
        );
        assert_eq!(classify(false, false, Tracking::Ahead(3)).0, State::Local);
        assert_eq!(classify(false, false, Tracking::Untracked).0, State::Local);
        assert_eq!(classify(false, false, Tracking::Gone).0, State::Local);
    }

    #[test]
    fn the_note_says_how_many_commits_are_at_stake() {
        let (_, note) = classify(false, false, Tracking::Ahead(3));
        assert!(note.contains('3'), "{note}");
    }
}
