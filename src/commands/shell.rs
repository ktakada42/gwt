//! `gwx shell-init` / `gwx completion` — shell integration.

use anyhow::Result;

use crate::cli::{Shell, ShellArgs};

/// Environment variable the completion stubs use to ask gwx for candidates.
pub const COMPLETE_VAR: &str = "COMPLETE";

/// Wrapper function that turns a chosen worktree into a real directory change.
///
/// A child process cannot move its parent shell, so `gwx` writes the directory
/// it wants into the file named by `GWX_CD_FILE` and the function below reads
/// it. Going through a file rather than stdout leaves `gwx list` free to print
/// its table, and keeps `gwx list --paths | peco` streaming as before.
///
/// Only the subcommands that can ask for a `cd` pay for the temporary file.
const POSIX_FUNCTION: &str = r#"
gwx() {
    case "${1-}" in
        cd|list|ls)
            __gwx_file="$(mktemp "${TMPDIR:-/tmp}/gwx-cd.XXXXXX")" || return 1
            GWX_CD_FILE="$__gwx_file" command gwx "$@"
            __gwx_status=$?
            __gwx_target="$(cat "$__gwx_file" 2>/dev/null)"
            rm -f "$__gwx_file"
            unset __gwx_file
            if [ "$__gwx_status" -eq 0 ] && [ -n "$__gwx_target" ]; then
                cd "$__gwx_target" || { unset __gwx_target; return 1; }
            fi
            unset __gwx_target
            return "$__gwx_status"
            ;;
        *)
            command gwx "$@"
            ;;
    esac
}
"#;

const FISH_FUNCTION: &str = r#"
function gwx --wraps gwx --description 'git worktree manager'
    if test (count $argv) -ge 1; and contains -- "$argv[1]" cd list ls
        set -l __gwx_file (mktemp "$TMPDIR"/gwx-cd.XXXXXX 2>/dev/null; or mktemp /tmp/gwx-cd.XXXXXX)
        GWX_CD_FILE=$__gwx_file command gwx $argv
        set -l __gwx_status $status
        set -l __gwx_target (cat $__gwx_file 2>/dev/null)
        rm -f $__gwx_file
        if test $__gwx_status -eq 0; and test -n "$__gwx_target"
            cd $__gwx_target
        end
        return $__gwx_status
    else
        command gwx $argv
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
/// what lets `gwx cd <TAB>` offer worktree names — so what is written here is
/// only a small stub that calls back into `COMPLETE=<shell> gwx`.
pub fn completion(args: ShellArgs) -> Result<()> {
    let mut out = Vec::new();
    args.shell
        .completer()
        .write_registration(COMPLETE_VAR, "gwx", "gwx", "gwx", &mut out)?;
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
            .write_registration(COMPLETE_VAR, "gwx", "gwx", "gwx", &mut out)
            .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn registration_is_generated_for_every_shell() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let script = generated(shell);
            assert!(script.contains("gwx"), "empty completion for {shell:?}");
            assert!(
                script.contains(COMPLETE_VAR),
                "{shell:?} script does not call back into gwx"
            );
        }
    }

    #[test]
    fn wrapper_only_intercepts_the_subcommands_that_can_move_you() {
        assert!(POSIX_FUNCTION.contains("cd|list|ls)"));
        assert!(FISH_FUNCTION.contains(r#"contains -- "$argv[1]" cd list ls"#));
        // Everything else must reach the binary untouched.
        assert!(POSIX_FUNCTION.contains(r#"command gwx "$@""#));
        assert!(FISH_FUNCTION.contains("command gwx $argv"));
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
