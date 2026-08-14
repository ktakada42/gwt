//! `.gwx.toml` — configuration, per repository and per user.
//!
//! Two files, same format. The repository's belongs to the project and is
//! meant to be committed; the user's holds what is nobody else's business —
//! where you like your worktrees, what you run in every checkout.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const CONFIG_FILE: &str = ".gwx.toml";

/// The user-wide file, under `$XDG_CONFIG_HOME` or `~/.config`.
pub const GLOBAL_CONFIG_PATH: &str = "gwx/config.toml";

/// Values accepted for the top-level `version` key.
const SUPPORTED_VERSIONS: &[&str] = &["1", "1.0"];

/// Default location of new worktrees, relative to the main worktree.
pub const DEFAULT_BASE_DIR: &str = "../worktrees";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Config format version. Currently informational only.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub hooks: Hooks,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Where worktrees are created, relative to the main worktree (or absolute).
    ///
    /// `None` rather than the default string, so that merging can tell a
    /// repository that chose `../worktrees` from one that said nothing and
    /// should inherit whatever the user prefers.
    #[serde(default)]
    pub base_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    /// Run before the worktree is created. Only `command` hooks are allowed,
    /// and they run in the main worktree.
    #[serde(default)]
    pub pre_create: Vec<Hook>,
    /// Run after the worktree is created, inside the new worktree.
    #[serde(default)]
    pub post_create: Vec<Hook>,
    /// Run before the worktree is removed, inside it while it is still there.
    /// Only `command` hooks are allowed. A failure calls the removal off.
    #[serde(default)]
    pub pre_remove: Vec<Hook>,
    /// Run after the worktree (and its branch, with `--with-branch`) is gone.
    /// Only `command` hooks are allowed, and they run in the main worktree.
    #[serde(default)]
    pub post_remove: Vec<Hook>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Hook {
    /// Copy a file or directory from the main worktree into the new one.
    Copy { from: String, to: Option<String> },
    /// Symlink a path in the main worktree from the new worktree.
    Symlink { from: String, to: Option<String> },
    /// Run a shell command.
    Command {
        command: String,
        #[serde(default)]
        env: BTreeMap<String, String>,
        /// Working directory, relative to the worktree the hook runs in.
        work_dir: Option<String>,
    },
}

impl Hook {
    pub fn kind(&self) -> &'static str {
        match self {
            Hook::Copy { .. } => "copy",
            Hook::Symlink { .. } => "symlink",
            Hook::Command { .. } => "command",
        }
    }

    /// One-line description used in progress output.
    pub fn summary(&self) -> String {
        match self {
            Hook::Copy { from, to } => format!("copy {from} -> {}", to.as_deref().unwrap_or(from)),
            Hook::Symlink { from, to } => {
                format!("symlink {from} -> {}", to.as_deref().unwrap_or(from))
            }
            Hook::Command { command, .. } => one_line(command),
        }
    }
}

/// A command squeezed into one progress line.
///
/// Hooks are numbered as they run — `[1/2] npm install` — and a command with a
/// newline in it, which a user-wide config makes more likely, would otherwise
/// break that list across the screen. Plain ASCII, like everything else gwx
/// draws.
fn one_line(command: &str) -> String {
    const WIDTH: usize = 60;

    let mut lines = command.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or_default();
    let head: String = first.chars().take(WIDTH).collect();
    if first.chars().count() > WIDTH || lines.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

impl Config {
    pub fn path_in(main_worktree: &Path) -> PathBuf {
        main_worktree.join(CONFIG_FILE)
    }

    /// Loads `.gwx.toml` from the main worktree, falling back to defaults when
    /// the file does not exist.
    pub fn load(main_worktree: &Path) -> Result<Self> {
        let path = Self::path_in(main_worktree);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                Self::parse(&text).with_context(|| format!("failed to parse {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text)?;
        if let Some(version) = &config.version {
            if !SUPPORTED_VERSIONS.contains(&version.as_str()) {
                bail!(
                    "unsupported config version `{version}` (this build understands {})",
                    SUPPORTED_VERSIONS.join(", ")
                );
            }
        }
        Ok(config)
    }

    /// Path of the user-wide file, or `None` when there is no home to look in.
    pub fn global_path() -> Option<PathBuf> {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => {
                let home = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .filter(|home| !home.is_empty())?;
                PathBuf::from(home).join(".config")
            }
        };
        Some(base.join(GLOBAL_CONFIG_PATH))
    }

    /// Loads the user-wide file, falling back to defaults when there is none.
    pub fn load_global() -> Result<Self> {
        let Some(path) = Self::global_path() else {
            return Ok(Self::default());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                Self::parse(&text).with_context(|| format!("failed to parse {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    /// The configuration in force: the repository's laid over the user's.
    ///
    /// Settings are merged key by key, so a repository that says nothing about
    /// `base_dir` inherits the user's rather than snapping back to the
    /// built-in default.
    ///
    /// Hooks are not overridden but run in addition, and the order is the one
    /// setup and teardown always take: creation runs the user's hooks first
    /// and the repository's second, removal runs them in reverse, so whatever
    /// a hook set up is still standing when the hook that undoes it runs.
    pub fn merge(user: Self, repo: Self) -> Self {
        fn chain(first: Vec<Hook>, second: Vec<Hook>) -> Vec<Hook> {
            first.into_iter().chain(second).collect()
        }

        Self {
            version: repo.version.or(user.version),
            defaults: Defaults {
                base_dir: repo.defaults.base_dir.or(user.defaults.base_dir),
            },
            hooks: Hooks {
                pre_create: chain(user.hooks.pre_create, repo.hooks.pre_create),
                post_create: chain(user.hooks.post_create, repo.hooks.post_create),
                pre_remove: chain(repo.hooks.pre_remove, user.hooks.pre_remove),
                post_remove: chain(repo.hooks.post_remove, user.hooks.post_remove),
            },
        }
    }

    /// Absolute base directory for new worktrees.
    pub fn base_dir(&self, main_worktree: &Path) -> PathBuf {
        let base = Path::new(
            self.defaults
                .base_dir
                .as_deref()
                .unwrap_or(DEFAULT_BASE_DIR),
        );
        if base.is_absolute() {
            base.to_path_buf()
        } else {
            normalize(&main_worktree.join(base))
        }
    }
}

/// Resolves `.` and `..` lexically, without touching the filesystem.
///
/// The path may not exist yet, so `canonicalize` is not an option.
pub fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub const TEMPLATE: &str = r#"# gwx configuration for this repository, meant to be committed.
# Settings that are yours rather than the project's go in the same format in
# ~/.config/gwx/config.toml, and this file wins where the two overlap.
# Docs: https://github.com/ktakada42/gwx
version = "1"

[defaults]
# Where worktrees are created, relative to the main worktree.
base_dir = "../worktrees"

# Hooks run before the worktree is created (command hooks only, run in the
# main worktree).
# [[hooks.pre_create]]
# type = "command"
# command = "echo creating $GWX_BRANCH"

# Hooks run after the worktree is created, inside the new worktree.
# [[hooks.post_create]]
# type = "copy"
# from = ".env"
# to = ".env"
#
# `symlink` shares one directory with the main worktree; `copy` gives the
# worktree its own, keeps symlinks as symlinks, and on macOS clones the tree
# rather than duplicating it.
# [[hooks.post_create]]
# type = "symlink"
# from = "node_modules"
#
# [[hooks.post_create]]
# type = "command"
# command = "npm install"
# work_dir = "."
# env = { NODE_ENV = "development" }

# Hooks run before the worktree is removed, inside it (command hooks only).
# A failing one calls the removal off.
# [[hooks.pre_remove]]
# type = "command"
# command = "docker compose down"

# Hooks run after the worktree is gone (command hooks only, run in the main
# worktree). GWX_WORKTREE_PATH still names the directory that was removed.
# [[hooks.post_remove]]
# type = "command"
# command = "rm -rf \"$GWX_MAIN_WORKTREE/.cache/$GWX_WORKTREE_NAME\""
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        let cfg = Config::parse("").unwrap();
        // Unset rather than defaulted, so a repository saying nothing can
        // inherit the user's choice.
        assert_eq!(cfg.defaults.base_dir, None);
        assert_eq!(cfg.base_dir(Path::new("/repo")), Path::new("/worktrees"));
        assert!(cfg.hooks.pre_create.is_empty());
        assert!(cfg.hooks.post_create.is_empty());
    }

    #[test]
    fn parses_all_hook_types() {
        let cfg = Config::parse(
            r#"
            version = "1"
            [defaults]
            base_dir = "../wt"

            [[hooks.pre_create]]
            type = "command"
            command = "echo hi"

            [[hooks.post_create]]
            type = "copy"
            from = ".env"

            [[hooks.post_create]]
            type = "symlink"
            from = "node_modules"
            to = "node_modules"

            [[hooks.post_create]]
            type = "command"
            command = "npm ci"
            work_dir = "app"
            env = { NODE_ENV = "development" }
            "#,
        )
        .unwrap();

        assert_eq!(cfg.defaults.base_dir.as_deref(), Some("../wt"));
        assert_eq!(cfg.hooks.pre_create.len(), 1);
        assert_eq!(cfg.hooks.post_create.len(), 3);
        assert_eq!(cfg.hooks.post_create[0].kind(), "copy");
        assert_eq!(cfg.hooks.post_create[1].kind(), "symlink");
        match &cfg.hooks.post_create[2] {
            Hook::Command { env, work_dir, .. } => {
                assert_eq!(env.get("NODE_ENV").map(String::as_str), Some("development"));
                assert_eq!(work_dir.as_deref(), Some("app"));
            }
            other => panic!("unexpected hook: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_versions() {
        assert!(Config::parse("version = \"2\"").is_err());
        assert!(Config::parse("version = \"1.0\"").is_ok());
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(Config::parse("[defaults]\nbasedir = \"x\"").is_err());
    }

    #[test]
    fn template_is_valid() {
        Config::parse(TEMPLATE).unwrap();
    }

    #[test]
    fn a_repository_that_says_nothing_inherits_the_users_base_dir() {
        let user = Config::parse("[defaults]\nbase_dir = \".worktrees\"").unwrap();
        let repo =
            Config::parse("[[hooks.post_create]]\ntype = \"copy\"\nfrom = \".env\"").unwrap();

        let merged = Config::merge(user, repo);
        assert_eq!(
            merged.base_dir(Path::new("/repo")),
            PathBuf::from("/repo/.worktrees")
        );
    }

    #[test]
    fn a_repository_that_chooses_one_keeps_it() {
        let user = Config::parse("[defaults]\nbase_dir = \".worktrees\"").unwrap();
        let repo = Config::parse("[defaults]\nbase_dir = \"../elsewhere\"").unwrap();

        let merged = Config::merge(user, repo);
        assert_eq!(
            merged.base_dir(Path::new("/repo")),
            PathBuf::from("/elsewhere")
        );
    }

    #[test]
    fn hooks_from_both_files_run_setup_forwards_and_teardown_backwards() {
        let user = Config::parse(
            r#"
            [[hooks.post_create]]
            type = "command"
            command = "user"

            [[hooks.pre_remove]]
            type = "command"
            command = "user"
            "#,
        )
        .unwrap();
        let repo = Config::parse(
            r#"
            [[hooks.post_create]]
            type = "command"
            command = "repo"

            [[hooks.pre_remove]]
            type = "command"
            command = "repo"
            "#,
        )
        .unwrap();

        let merged = Config::merge(user, repo);
        let summaries = |hooks: &[Hook]| hooks.iter().map(Hook::summary).collect::<Vec<_>>();

        // Creating: the user's environment first, then what the project adds.
        assert_eq!(summaries(&merged.hooks.post_create), ["user", "repo"]);
        // Removing: the project's teardown first, since the user's set-up is
        // the ground it stands on.
        assert_eq!(summaries(&merged.hooks.pre_remove), ["repo", "user"]);
    }

    #[test]
    fn a_command_summary_stays_on_one_line() {
        let summary = |command: &str| {
            Hook::Command {
                command: command.into(),
                env: Default::default(),
                work_dir: None,
            }
            .summary()
        };

        assert_eq!(summary("npm install"), "npm install");
        assert_eq!(summary("  npm install  "), "npm install");
        assert_eq!(summary("set -e\nnpm install\n"), "set -e...");
        assert_eq!(summary(&"x".repeat(80)), format!("{}...", "x".repeat(60)));
        for command in ["npm install", "set -e\nnpm install", &"x".repeat(80)] {
            assert!(!summary(command).contains('\n'));
            assert!(summary(command).is_ascii());
        }
    }

    #[test]
    fn the_global_path_follows_xdg_then_home() {
        // Both variables are read at call time, so this only checks the shape
        // of what is built from them.
        let path = Config::global_path().expect("HOME is set in tests");
        assert!(path.ends_with("gwx/config.toml"), "{}", path.display());
    }

    #[test]
    fn base_dir_is_resolved_against_the_main_worktree() {
        let cfg = Config::parse("").unwrap();
        assert_eq!(
            cfg.base_dir(Path::new("/home/me/repo")),
            PathBuf::from("/home/me/worktrees")
        );

        let cfg = Config::parse("[defaults]\nbase_dir = \"/tmp/wt\"").unwrap();
        assert_eq!(
            cfg.base_dir(Path::new("/home/me/repo")),
            PathBuf::from("/tmp/wt")
        );
    }

    #[test]
    fn normalize_resolves_dots() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(normalize(Path::new("../a")), PathBuf::from("../a"));
    }
}
