//! Handing a directory back to the shell.
//!
//! A process cannot change its parent's working directory, so `gwx` has to ask
//! the shell function from `gwx shell-init` to do it.
//!
//! The request travels through a file named by `GWX_CD_FILE` rather than
//! stdout. Stdout would work for `gwx cd`, whose only output *is* the path, but
//! not for `gwx list`, which prints a table the shell must not treat as a
//! destination. Keeping stdout free also means `gwx list --paths | peco` still
//! streams and pipes exactly as it did.

use std::ffi::OsString;
use std::path::Path;

use anyhow::{Context, Result};

/// Environment variable holding the path of the hand-off file.
pub const CD_FILE_VAR: &str = "GWX_CD_FILE";

/// Asks the shell to move into `path`.
pub fn request(path: &Path) -> Result<()> {
    write_request(std::env::var_os(CD_FILE_VAR), path)
}

/// Asks the shell to move into a worktree the user just picked from the list.
///
/// Printing a path is a fine answer for `gwx cd feature/x`, which someone may
/// have wrapped in a command substitution. It is a dead end after the picker:
/// the user chose a destination and nothing happened. That almost always means
/// the shell function predates the hand-off file, so say so.
pub fn request_picked(path: &Path) -> Result<()> {
    let file = std::env::var_os(CD_FILE_VAR);
    let integrated = file.as_ref().is_some_and(|f| !f.is_empty());
    write_request(file, path)?;
    if !integrated {
        eprintln!(
            "gwx: could not change directory — the shell integration is missing or out of date."
        );
        eprintln!("     Start a new shell, or reload it with: eval \"$(gwx shell-init zsh)\"");
    }
    Ok(())
}

/// Writes the request to `file`, falling back to stdout when there is none.
///
/// Without the shell integration nobody is reading the file, so the path goes
/// to stdout instead and `cd "$(gwx cd feature/x)"` keeps working.
fn write_request(file: Option<OsString>, path: &Path) -> Result<()> {
    match file {
        Some(file) if !file.is_empty() => std::fs::write(&file, format!("{}\n", path.display()))
            .with_context(|| {
                format!(
                    "failed to write the target directory to {}",
                    Path::new(&file).display()
                )
            }),
        _ => {
            println!("{}", path.display());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_to_the_hand_off_file_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cd");

        write_request(
            Some(file.clone().into_os_string()),
            Path::new("/home/me/worktrees/feature/auth"),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "/home/me/worktrees/feature/auth\n"
        );
    }

    #[test]
    fn falls_back_to_stdout_without_a_file() {
        // stdout is what the end-to-end tests assert on; here we only check
        // that both "unset" and "set but empty" take the fallback path.
        write_request(None, Path::new("/tmp")).unwrap();
        write_request(Some(OsString::new()), Path::new("/tmp")).unwrap();
    }

    #[test]
    fn reports_an_unwritable_hand_off_file() {
        let err = write_request(
            Some(OsString::from("/nonexistent-dir/gwx-cd")),
            Path::new("/tmp"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("failed to write"), "{err}");
    }
}
