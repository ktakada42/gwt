//! Execution of the hooks declared in `.gwt.toml`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::config::{normalize, Hook};

/// Everything a hook is allowed to know about the worktree being created.
pub struct HookContext {
    pub main_worktree: PathBuf,
    pub worktree_path: PathBuf,
    /// Worktree name as used on the command line (branch name).
    pub name: String,
    pub branch: String,
}

impl HookContext {
    fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            (
                "GWT_MAIN_WORKTREE",
                self.main_worktree.display().to_string(),
            ),
            (
                "GWT_WORKTREE_PATH",
                self.worktree_path.display().to_string(),
            ),
            ("GWT_WORKTREE_NAME", self.name.clone()),
            ("GWT_BRANCH", self.branch.clone()),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    PreCreate,
    PostCreate,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::PreCreate => "pre_create",
            Phase::PostCreate => "post_create",
        }
    }
}

/// Runs every hook of a phase in order, stopping at the first failure.
///
/// Progress goes to stderr so that stdout stays usable for `--quiet` output.
pub fn run_all(hooks: &[Hook], phase: Phase, ctx: &HookContext, verbose: bool) -> Result<()> {
    if hooks.is_empty() {
        return Ok(());
    }
    if verbose {
        eprintln!("Running {} hooks...", phase.label());
    }
    for (i, hook) in hooks.iter().enumerate() {
        if verbose {
            eprintln!("  [{}/{}] {}", i + 1, hooks.len(), hook.summary());
        }
        run_one(hook, phase, ctx)
            .with_context(|| format!("{} hook failed: {}", phase.label(), hook.summary()))?;
    }
    Ok(())
}

fn run_one(hook: &Hook, phase: Phase, ctx: &HookContext) -> Result<()> {
    match hook {
        Hook::Copy { .. } | Hook::Symlink { .. } if phase == Phase::PreCreate => {
            bail!(
                "`{}` hooks are only supported in post_create (the worktree does not exist yet)",
                hook.kind()
            )
        }
        Hook::Copy { from, to } => {
            let src = resolve_inside(&ctx.main_worktree, from)?;
            let dst = resolve_inside(&ctx.worktree_path, to.as_deref().unwrap_or(from))?;
            if !src.exists() {
                bail!("source does not exist: {}", src.display());
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            copy_recursive(&src, &dst)
                .with_context(|| format!("copying {} to {}", src.display(), dst.display()))
        }
        Hook::Symlink { from, to } => {
            let src = resolve_inside(&ctx.main_worktree, from)?;
            let dst = resolve_inside(&ctx.worktree_path, to.as_deref().unwrap_or(from))?;
            if !src.exists() {
                bail!("source does not exist: {}", src.display());
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if dst.symlink_metadata().is_ok() {
                bail!("destination already exists: {}", dst.display());
            }
            symlink(&src, &dst)
                .with_context(|| format!("linking {} to {}", dst.display(), src.display()))
        }
        Hook::Command {
            command,
            env,
            work_dir,
        } => {
            let base = match phase {
                Phase::PreCreate => &ctx.main_worktree,
                Phase::PostCreate => &ctx.worktree_path,
            };
            let cwd = match work_dir {
                Some(dir) => resolve_inside(base, dir)?,
                None => base.clone(),
            };
            let mut cmd = shell_command(command);
            cmd.current_dir(&cwd);
            for (k, v) in ctx.env() {
                cmd.env(k, v);
            }
            for (k, v) in env {
                cmd.env(k, v);
            }
            let status = cmd
                .status()
                .with_context(|| format!("failed to spawn command: {command}"))?;
            if !status.success() {
                bail!("command exited with status {status}");
            }
            Ok(())
        }
    }
}

/// Commands always run through `/bin/sh` rather than `$SHELL`, so that a hook
/// behaves the same for everyone who checks out the repository.
///
/// A hook runs *in* the new worktree, so it starts without the variables that
/// would point git somewhere else — otherwise `git submodule update` in a
/// post_create hook would quietly operate on whatever repository invoked gwt.
#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(command);
    for var in crate::git::REPO_ENV {
        cmd.env_remove(var);
    }
    cmd
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(command);
    cmd
}

#[cfg(unix)]
fn symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(windows)]
fn symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::os::windows::fs::symlink_dir(src, dst)
    } else {
        std::os::windows::fs::symlink_file(src, dst)
    }
}

/// Joins `rel` onto `base` and refuses to escape it.
///
/// Hook paths come from a file in the repository, so a stray `../..` should be
/// reported rather than silently writing outside the worktree.
fn resolve_inside(base: &Path, rel: &str) -> Result<PathBuf> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() {
        bail!("absolute paths are not allowed in hooks: {rel}");
    }
    let joined = normalize(&base.join(candidate));
    if !joined.starts_with(base) {
        bail!("path escapes the worktree: {rel}");
    }
    Ok(joined)
}

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_inside_accepts_nested_paths() {
        let base = Path::new("/repo");
        assert_eq!(
            resolve_inside(base, "a/b.txt").unwrap(),
            PathBuf::from("/repo/a/b.txt")
        );
        assert_eq!(
            resolve_inside(base, "./a").unwrap(),
            PathBuf::from("/repo/a")
        );
        assert_eq!(
            resolve_inside(base, "a/../b").unwrap(),
            PathBuf::from("/repo/b")
        );
    }

    #[test]
    fn resolve_inside_rejects_escapes() {
        let base = Path::new("/repo");
        assert!(resolve_inside(base, "../outside").is_err());
        assert!(resolve_inside(base, "a/../../outside").is_err());
        assert!(resolve_inside(base, "/etc/passwd").is_err());
    }

    #[test]
    fn copy_and_symlink_are_rejected_before_creation() {
        let ctx = HookContext {
            main_worktree: PathBuf::from("/repo"),
            worktree_path: PathBuf::from("/wt"),
            name: "feat".into(),
            branch: "feat".into(),
        };
        let hook = Hook::Copy {
            from: ".env".into(),
            to: None,
        };
        assert!(run_one(&hook, Phase::PreCreate, &ctx).is_err());
    }

    #[test]
    fn copies_files_and_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(main.join("cfg")).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(main.join(".env"), "SECRET=1").unwrap();
        std::fs::write(main.join("cfg/a.txt"), "a").unwrap();

        let ctx = HookContext {
            main_worktree: main.clone(),
            worktree_path: wt.clone(),
            name: "feat".into(),
            branch: "feat".into(),
        };

        run_one(
            &Hook::Copy {
                from: ".env".into(),
                to: None,
            },
            Phase::PostCreate,
            &ctx,
        )
        .unwrap();
        run_one(
            &Hook::Copy {
                from: "cfg".into(),
                to: Some("config".into()),
            },
            Phase::PostCreate,
            &ctx,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(wt.join(".env")).unwrap(),
            "SECRET=1"
        );
        assert_eq!(
            std::fs::read_to_string(wt.join("config/a.txt")).unwrap(),
            "a"
        );
    }

    #[test]
    fn command_hook_runs_in_the_new_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&wt).unwrap();

        let ctx = HookContext {
            main_worktree: main,
            worktree_path: wt.clone(),
            name: "feat".into(),
            branch: "feat".into(),
        };

        run_one(
            &Hook::Command {
                command: "printf %s \"$GWT_BRANCH\" > marker".into(),
                env: Default::default(),
                work_dir: None,
            },
            Phase::PostCreate,
            &ctx,
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(wt.join("marker")).unwrap(), "feat");
    }

    #[test]
    fn failing_command_hook_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = HookContext {
            main_worktree: tmp.path().to_path_buf(),
            worktree_path: tmp.path().to_path_buf(),
            name: "feat".into(),
            branch: "feat".into(),
        };
        let hook = Hook::Command {
            command: "exit 3".into(),
            env: Default::default(),
            work_dir: None,
        };
        assert!(run_one(&hook, Phase::PostCreate, &ctx).is_err());
    }
}
