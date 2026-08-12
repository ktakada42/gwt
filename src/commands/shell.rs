//! `gwt shell-init` / `gwt completion` — shell integration.

use anyhow::Result;

use crate::cli::{Shell, ShellArgs};

/// Environment variable the completion stubs use to ask gwt for candidates.
pub const COMPLETE_VAR: &str = "COMPLETE";

/// Wrapper function that turns a chosen worktree into a real directory change.
///
/// A child process cannot move its parent shell, so `gwt` writes the directory
/// it wants into the file named by `GWT_CD_FILE` and the function below reads
/// it. Going through a file rather than stdout leaves `gwt list` free to print
/// its table, and keeps `gwt list --paths | peco` streaming as before.
///
/// Only the subcommands that can ask for a `cd` pay for the temporary file.
const POSIX_FUNCTION: &str = r#"
gwt() {
    case "${1-}" in
        cd|list|ls)
            __gwt_file="$(mktemp "${TMPDIR:-/tmp}/gwt-cd.XXXXXX")" || return 1
            GWT_CD_FILE="$__gwt_file" command gwt "$@"
            __gwt_status=$?
            __gwt_target="$(cat "$__gwt_file" 2>/dev/null)"
            rm -f "$__gwt_file"
            unset __gwt_file
            if [ "$__gwt_status" -eq 0 ] && [ -n "$__gwt_target" ]; then
                cd "$__gwt_target" || { unset __gwt_target; return 1; }
            fi
            unset __gwt_target
            return "$__gwt_status"
            ;;
        *)
            command gwt "$@"
            ;;
    esac
}
"#;

const FISH_FUNCTION: &str = r#"
function gwt --wraps gwt --description 'git worktree manager'
    if test (count $argv) -ge 1; and contains -- "$argv[1]" cd list ls
        set -l __gwt_file (mktemp "$TMPDIR"/gwt-cd.XXXXXX 2>/dev/null; or mktemp /tmp/gwt-cd.XXXXXX)
        GWT_CD_FILE=$__gwt_file command gwt $argv
        set -l __gwt_status $status
        set -l __gwt_target (cat $__gwt_file 2>/dev/null)
        rm -f $__gwt_file
        if test $__gwt_status -eq 0; and test -n "$__gwt_target"
            cd $__gwt_target
        end
        return $__gwt_status
    else
        command gwt $argv
    end
end
"#;

pub fn shell_init(args: ShellArgs) -> Result<()> {
    let function = match args.shell {
        Shell::Bash | Shell::Zsh => POSIX_FUNCTION,
        Shell::Fish => FISH_FUNCTION,
    };
    print!("{function}");
    println!();
    completion(args)
}

/// Emits the completion registration script.
///
/// Candidates are computed by the binary itself at completion time — that is
/// what lets `gwt cd <TAB>` offer worktree names — so what is written here is
/// only a small stub that calls back into `COMPLETE=<shell> gwt`.
pub fn completion(args: ShellArgs) -> Result<()> {
    let mut out = Vec::new();
    args.shell
        .completer()
        .write_registration(COMPLETE_VAR, "gwt", "gwt", "gwt", &mut out)?;
    print!("{}", String::from_utf8_lossy(&out));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated(shell: Shell) -> String {
        let mut out = Vec::new();
        shell
            .completer()
            .write_registration(COMPLETE_VAR, "gwt", "gwt", "gwt", &mut out)
            .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn registration_is_generated_for_every_shell() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let script = generated(shell);
            assert!(script.contains("gwt"), "empty completion for {shell:?}");
            assert!(
                script.contains(COMPLETE_VAR),
                "{shell:?} script does not call back into gwt"
            );
        }
    }

    #[test]
    fn wrapper_only_intercepts_the_subcommands_that_can_move_you() {
        assert!(POSIX_FUNCTION.contains("cd|list|ls)"));
        assert!(FISH_FUNCTION.contains(r#"contains -- "$argv[1]" cd list ls"#));
        // Everything else must reach the binary untouched.
        assert!(POSIX_FUNCTION.contains(r#"command gwt "$@""#));
        assert!(FISH_FUNCTION.contains("command gwt $argv"));
    }

    #[test]
    fn wrapper_hands_the_directory_over_through_a_file() {
        for script in [POSIX_FUNCTION, FISH_FUNCTION] {
            assert!(script.contains(crate::cd_target::CD_FILE_VAR), "{script}");
            assert!(script.contains("mktemp"), "{script}");
            // The file must be cleaned up whether or not a cd happens.
            assert!(script.contains("rm -f"), "{script}");
        }
    }
}
