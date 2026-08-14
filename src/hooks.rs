//! Execution of the hooks declared in `.gwx.toml`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::config::{normalize, Hook};

/// Everything a hook is allowed to know about the worktree being created or
/// removed.
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
                "GWX_MAIN_WORKTREE",
                self.main_worktree.display().to_string(),
            ),
            (
                "GWX_WORKTREE_PATH",
                self.worktree_path.display().to_string(),
            ),
            ("GWX_WORKTREE_NAME", self.name.clone()),
            ("GWX_BRANCH", self.branch.clone()),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    PreCreate,
    PostCreate,
    PreRemove,
    PostRemove,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::PreCreate => "pre_create",
            Phase::PostCreate => "post_create",
            Phase::PreRemove => "pre_remove",
            Phase::PostRemove => "post_remove",
        }
    }

    /// Why this phase has nothing to copy into or link from.
    ///
    /// Only `post_create` hands a hook a worktree that is both there and
    /// still wanted; the other three are told apart in the error so a
    /// misplaced `copy` says what is actually wrong with it.
    fn no_worktree_reason(self) -> &'static str {
        match self {
            Phase::PreCreate => "the worktree does not exist yet",
            Phase::PostCreate => unreachable!("post_create has a worktree"),
            Phase::PreRemove => "the worktree is about to be deleted",
            Phase::PostRemove => "the worktree is already gone",
        }
    }
}

/// How much of a hook run reaches the terminal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reporting {
    /// Name each hook as it starts, and let it write to the terminal.
    Announce,
    /// Say nothing, but still let hooks write to the terminal (`--quiet`).
    Quiet,
    /// Say nothing and capture what hooks print, for the picker: it owns an
    /// alternate screen in raw mode, where stray output lands in the middle
    /// of the list with its line breaks mangled. A failing hook's last words
    /// are put in the error instead, which is what the picker shows.
    Captured,
}

/// Runs every hook of a phase in order, stopping at the first failure.
///
/// Progress goes to stderr so that stdout stays usable for `--quiet` output.
pub fn run_all(
    hooks: &[Hook],
    phase: Phase,
    ctx: &HookContext,
    reporting: Reporting,
) -> Result<()> {
    if hooks.is_empty() {
        return Ok(());
    }
    let announce = reporting == Reporting::Announce;
    if announce {
        eprintln!("Running {} hooks...", phase.label());
    }
    for (i, hook) in hooks.iter().enumerate() {
        let position = format!("{}/{}", i + 1, hooks.len());
        if announce {
            eprintln!("  [{position}] {}", hook.summary());
        }
        if let Err(e) = run_one(hook, phase, ctx, reporting) {
            // The numbered list above already shows what ran. What it cannot
            // show is that the rest never will, which is the difference
            // between a worktree that is set up and one that stopped halfway.
            let left = hooks.len() - (i + 1);
            if announce && left > 0 {
                eprintln!(
                    "        stopped here; {left} of {} did not run",
                    hooks.len()
                );
            }
            return Err(e).with_context(|| {
                format!(
                    "{} hook {position} failed: {}",
                    phase.label(),
                    hook.summary()
                )
            });
        }
    }
    Ok(())
}

fn run_one(hook: &Hook, phase: Phase, ctx: &HookContext, reporting: Reporting) -> Result<()> {
    match hook {
        Hook::Copy { .. } | Hook::Symlink { .. } if phase != Phase::PostCreate => {
            bail!(
                "`{}` hooks are only supported in post_create ({})",
                hook.kind(),
                phase.no_worktree_reason()
            )
        }
        Hook::Copy { from, to } => {
            let src = resolve_inside(&ctx.main_worktree, from)?;
            let dst = resolve_inside(&ctx.worktree_path, to.as_deref().unwrap_or(from))?;
            // `symlink_metadata` rather than `exists`, which follows the link
            // and calls a symlink pointing at nothing a missing source.
            if src.symlink_metadata().is_err() {
                bail!("source does not exist: {}", src.display());
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            copy_tree(&src, &dst)
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
            // A hook runs where its work is: in the worktree while there is
            // one, in the main worktree before it exists and after it is gone.
            let base = match phase {
                Phase::PostCreate | Phase::PreRemove => &ctx.worktree_path,
                Phase::PreCreate | Phase::PostRemove => &ctx.main_worktree,
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
            if reporting == Reporting::Captured {
                let out = cmd
                    .output()
                    .with_context(|| format!("failed to spawn command: {command}"))?;
                if !out.status.success() {
                    bail!(
                        "command exited with status {}{}",
                        exit_status(&out.status),
                        tail(&out.stderr)
                    );
                }
                return Ok(());
            }
            let status = cmd
                .status()
                .with_context(|| format!("failed to spawn command: {command}"))?;
            if !status.success() {
                bail!("command exited with status {}", exit_status(&status));
            }
            Ok(())
        }
    }
}

/// How a hook ended, without `ExitStatus`'s own wording.
///
/// Its `Display` is "exit status: 7", which lands in a sentence that already
/// says "status" and reads as "status exit status: 7".
fn exit_status(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => code.to_string(),
        // Killed by a signal, where the default wording is the useful one.
        None => status.to_string(),
    }
}

/// The last few lines a captured hook wrote to stderr.
///
/// Captured output is otherwise thrown away, and "exited with status 1" on its
/// own tells you nothing about which command gave up or why.
fn tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect();
    let start = lines.len().saturating_sub(3);
    if lines.is_empty() {
        String::new()
    } else {
        format!(": {}", lines[start..].join("; "))
    }
}

/// Commands always run through `/bin/sh` rather than `$SHELL`, so that a hook
/// behaves the same for everyone who checks out the repository.
///
/// A hook runs *in* the new worktree, so it starts without the variables that
/// would point git somewhere else — otherwise `git submodule update` in a
/// post_create hook would quietly operate on whatever repository invoked gwx.
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

/// Copies a file, a directory tree, or a symlink, taking the fast path when
/// the platform has one.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    if src.is_dir() && clone_tree(src, dst) {
        return Ok(());
    }
    copy_recursive(src, dst)
}

/// Clones a whole directory tree in a single syscall.
///
/// `std::fs::copy` already clones each file on APFS, so this changes nothing
/// about disk usage — what it saves is the walk. A `node_modules` of 10,000
/// files took 2.2s through the recursion and 0.14s through one `clonefile`.
///
/// Any failure means the recursion runs instead: the destination already
/// exists (`EEXIST`, a copy merging into a directory that is there), the two
/// paths are on different volumes (`EXDEV`), or the filesystem is not APFS
/// (`ENOTSUP`). Those are the expected ones, and anything else is better
/// reported by the recursion, which knows which file it was on.
///
/// The two paths agree on what they produce: `clonefile` keeps symlinks as
/// symlinks, and so does `copy_recursive`.
#[cfg(target_os = "macos")]
fn clone_tree(src: &Path, dst: &Path) -> bool {
    use std::ffi::{c_char, c_int, CString};
    use std::os::unix::ffi::OsStrExt;

    extern "C" {
        fn clonefile(src: *const c_char, dst: *const c_char, flags: c_int) -> c_int;
    }

    let (Ok(src), Ok(dst)) = (
        CString::new(src.as_os_str().as_bytes()),
        CString::new(dst.as_os_str().as_bytes()),
    ) else {
        // A path with an interior NUL, which the filesystem cannot hold
        // anyway. Let the recursion produce the error.
        return false;
    };
    // SAFETY: both arguments are NUL-terminated C strings that live until the
    // call returns, which is all `clonefile` asks of them.
    unsafe { clonefile(src.as_ptr(), dst.as_ptr(), 0) == 0 }
}

/// Copies a tree entry by entry, one `std::fs::copy` per file.
///
/// Symlinks are recreated as symlinks rather than followed. Dereferencing them
/// duplicates whatever they point at, breaks the relative ones — `node_modules`
/// is full of both — and turns a link pointing at nothing into a hard error in
/// the middle of a half-copied tree.
fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = src.symlink_metadata()?;
    if meta.is_symlink() {
        // `std::fs::copy` overwrites, so copying a tree twice has to be able
        // to replace a link it wrote last time.
        if dst.symlink_metadata().is_ok() {
            std::fs::remove_file(dst)?;
        }
        symlink(&std::fs::read_link(src)?, dst)
    } else if meta.is_dir() {
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
        assert!(run_one(&hook, Phase::PreCreate, &ctx, Reporting::Announce).is_err());
    }

    /// A `node_modules`-shaped tree: a link to a file, a link to a directory,
    /// and one pointing at nothing, which is what is left after a package is
    /// removed.
    fn tree_with_symlinks(root: &Path) {
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::fs::write(root.join("pkg/index.js"), "module.exports = 1").unwrap();
        symlink(Path::new("pkg/index.js"), &root.join("link-to-file")).unwrap();
        symlink(Path::new("pkg"), &root.join("link-to-dir")).unwrap();
        symlink(Path::new("nowhere.js"), &root.join("dangling")).unwrap();
    }

    fn assert_tree_was_preserved(root: &Path) {
        for (link, target) in [
            ("link-to-file", "pkg/index.js"),
            ("link-to-dir", "pkg"),
            ("dangling", "nowhere.js"),
        ] {
            let path = root.join(link);
            assert!(
                path.symlink_metadata().unwrap().is_symlink(),
                "{link} was dereferenced"
            );
            assert_eq!(std::fs::read_link(&path).unwrap(), Path::new(target));
        }
        assert_eq!(
            std::fs::read_to_string(root.join("pkg/index.js")).unwrap(),
            "module.exports = 1"
        );
    }

    #[test]
    fn copying_keeps_symlinks_as_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        tree_with_symlinks(&main.join("node_modules"));

        let ctx = HookContext {
            main_worktree: main,
            worktree_path: wt.clone(),
            name: "feat".into(),
            branch: "feat".into(),
        };

        // On macOS this takes the clonefile path; everywhere else, the walk.
        run_one(
            &Hook::Copy {
                from: "node_modules".into(),
                to: None,
            },
            Phase::PostCreate,
            &ctx,
            Reporting::Announce,
        )
        .unwrap();

        assert_tree_was_preserved(&wt.join("node_modules"));
    }

    #[test]
    fn the_walk_keeps_symlinks_too() {
        // The fast path is skipped here, so macOS also exercises the fallback
        // the two are supposed to agree with.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        tree_with_symlinks(&src);

        copy_recursive(&src, &dst).unwrap();
        assert_tree_was_preserved(&dst);

        // Copying again over the result replaces the links rather than
        // failing on the ones that are already there.
        copy_recursive(&src, &dst).unwrap();
        assert_tree_was_preserved(&dst);
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
            Reporting::Announce,
        )
        .unwrap();
        run_one(
            &Hook::Copy {
                from: "cfg".into(),
                to: Some("config".into()),
            },
            Phase::PostCreate,
            &ctx,
            Reporting::Announce,
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
                command: "printf %s \"$GWX_BRANCH\" > marker".into(),
                env: Default::default(),
                work_dir: None,
            },
            Phase::PostCreate,
            &ctx,
            Reporting::Announce,
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
        assert!(run_one(&hook, Phase::PostCreate, &ctx, Reporting::Announce).is_err());
    }

    #[test]
    fn copy_and_symlink_are_rejected_around_removal() {
        let ctx = HookContext {
            main_worktree: PathBuf::from("/repo"),
            worktree_path: PathBuf::from("/wt"),
            name: "feat".into(),
            branch: "feat".into(),
        };
        let hook = Hook::Symlink {
            from: "node_modules".into(),
            to: None,
        };
        for phase in [Phase::PreRemove, Phase::PostRemove] {
            let err = run_one(&hook, phase, &ctx, Reporting::Announce).unwrap_err();
            assert!(
                err.to_string().contains("only supported in post_create"),
                "unexpected error: {err}"
            );
        }
    }

    #[test]
    fn pre_remove_runs_in_the_worktree_and_post_remove_in_the_main_one() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&wt).unwrap();

        let ctx = HookContext {
            main_worktree: main.clone(),
            worktree_path: wt.clone(),
            name: "feat".into(),
            branch: "feat".into(),
        };
        let hook = Hook::Command {
            command: "pwd > where".into(),
            env: Default::default(),
            work_dir: None,
        };

        run_one(&hook, Phase::PreRemove, &ctx, Reporting::Announce).unwrap();
        run_one(&hook, Phase::PostRemove, &ctx, Reporting::Announce).unwrap();

        assert!(
            wt.join("where").exists(),
            "pre_remove ran outside the worktree"
        );
        assert!(
            main.join("where").exists(),
            "post_remove ran outside the main worktree"
        );
    }

    #[test]
    fn captured_output_ends_up_in_the_error() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = HookContext {
            main_worktree: tmp.path().to_path_buf(),
            worktree_path: tmp.path().to_path_buf(),
            name: "feat".into(),
            branch: "feat".into(),
        };
        let hook = Hook::Command {
            command: "echo nope >&2; exit 1".into(),
            env: Default::default(),
            work_dir: None,
        };

        let err = run_one(&hook, Phase::PreRemove, &ctx, Reporting::Captured).unwrap_err();
        assert!(err.to_string().contains("nope"), "unexpected error: {err}");
    }

    #[test]
    fn tail_keeps_the_last_lines_only() {
        assert_eq!(tail(b""), "");
        assert_eq!(tail(b"\n  \n"), "");
        assert_eq!(tail(b"one\ntwo\n"), ": one; two");
        assert_eq!(tail(b"1\n2\n3\n4\n5\n"), ": 3; 4; 5");
    }
}
