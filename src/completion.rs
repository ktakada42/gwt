//! Candidates for dynamic shell completion.
//!
//! These run in a separate process that the shell spawns on every `<TAB>`, so
//! they must be quick and must never fail: outside a repository, or when git
//! misbehaves, the answer is simply "no candidates".

use clap_complete::CompletionCandidate;

use crate::git;
use crate::repo::{Repo, MAIN_ALIAS};

/// Worktrees `gwt cd` accepts, including `@` for the main worktree.
pub fn worktrees() -> Vec<CompletionCandidate> {
    candidates(false)
}

/// Worktrees `gwt remove` accepts — everything but the main worktree.
pub fn removable_worktrees() -> Vec<CompletionCandidate> {
    candidates(true)
}

fn candidates(skip_main: bool) -> Vec<CompletionCandidate> {
    let Ok(repo) = Repo::discover() else {
        return Vec::new();
    };
    let Ok(worktrees) = repo.worktrees() else {
        return Vec::new();
    };
    let Some(main) = worktrees.first() else {
        return Vec::new();
    };

    worktrees
        .iter()
        .skip(usize::from(skip_main))
        .map(|wt| {
            let name = repo.display_name(wt, main);
            let hint = if name == MAIN_ALIAS {
                format!("main worktree — {}", wt.path.display())
            } else {
                wt.path.display().to_string()
            };
            CompletionCandidate::new(name).help(Some(hint.into()))
        })
        .collect()
}

/// Branches `gwt add` can turn into a worktree.
///
/// Branches that already have a worktree are left out — `gwt add` would refuse
/// them anyway. Remote-only branches are offered under their short name, which
/// is exactly what `gwt add` expects.
pub fn addable_branches() -> Vec<CompletionCandidate> {
    let Ok(repo) = Repo::discover() else {
        return Vec::new();
    };
    let Ok(worktrees) = repo.worktrees() else {
        return Vec::new();
    };
    let in_use: Vec<String> = worktrees.iter().filter_map(|w| w.branch.clone()).collect();

    let locals = git::local_branches(&repo.cwd).unwrap_or_default();
    let mut candidates: Vec<CompletionCandidate> = locals
        .iter()
        .filter(|b| !in_use.contains(b))
        .map(|b| CompletionCandidate::new(b).help(Some("local branch".into())))
        .collect();

    for (full, short) in git::remote_branches(&repo.cwd).unwrap_or_default() {
        if locals.contains(&short) || in_use.contains(&short) {
            continue;
        }
        candidates.push(CompletionCandidate::new(short).help(Some(full.into())));
    }
    candidates
}

/// Refs `gwt add --from` accepts: branches, tags and remote-tracking branches.
pub fn start_points() -> Vec<CompletionCandidate> {
    let Ok(repo) = Repo::discover() else {
        return Vec::new();
    };
    git::start_points(&repo.cwd)
        .unwrap_or_default()
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}
