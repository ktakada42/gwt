//! `.gwt.toml` — per-repository configuration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const CONFIG_FILE: &str = ".gwt.toml";

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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Where worktrees are created, relative to the main worktree (or absolute).
    #[serde(default = "default_base_dir")]
    pub base_dir: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            base_dir: default_base_dir(),
        }
    }
}

fn default_base_dir() -> String {
    DEFAULT_BASE_DIR.to_string()
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
            Hook::Command { command, .. } => command.clone(),
        }
    }
}

impl Config {
    pub fn path_in(main_worktree: &Path) -> PathBuf {
        main_worktree.join(CONFIG_FILE)
    }

    /// Loads `.gwt.toml` from the main worktree, falling back to defaults when
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

    /// Absolute base directory for new worktrees.
    pub fn base_dir(&self, main_worktree: &Path) -> PathBuf {
        let base = Path::new(&self.defaults.base_dir);
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

pub const TEMPLATE: &str = r#"# gwt configuration
# Docs: https://github.com/ktakada42/gwt
version = "1"

[defaults]
# Where worktrees are created, relative to the main worktree.
base_dir = "../worktrees"

# Hooks run before the worktree is created (command hooks only, run in the
# main worktree).
# [[hooks.pre_create]]
# type = "command"
# command = "echo creating $GWT_BRANCH"

# Hooks run after the worktree is created, inside the new worktree.
# [[hooks.post_create]]
# type = "copy"
# from = ".env"
# to = ".env"
#
# [[hooks.post_create]]
# type = "symlink"
# from = "node_modules"
#
# [[hooks.post_create]]
# type = "command"
# command = "npm install"
# work_dir = "."
# env = { NODE_ENV = "development" }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        let cfg = Config::parse("").unwrap();
        assert_eq!(cfg.defaults.base_dir, DEFAULT_BASE_DIR);
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

        assert_eq!(cfg.defaults.base_dir, "../wt");
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
