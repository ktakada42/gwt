//! `gwt shell-init` / `gwt completion` — shell integration.

use anyhow::Result;

use crate::cli::{Shell, ShellArgs};

/// Environment variable the completion stubs use to ask gwt for candidates.
pub const COMPLETE_VAR: &str = "COMPLETE";

/// Wrapper function that turns `gwt cd` into a real directory change.
///
/// POSIX shells run each command in a child process, so the binary itself can
/// only print the path; the function below consumes it.
const POSIX_FUNCTION: &str = r#"
gwt() {
    if [ "$1" = "cd" ]; then
        shift
        __gwt_target="$(command gwt cd "$@")" || return $?
        if [ -n "$__gwt_target" ]; then
            cd "$__gwt_target" || return $?
        fi
        unset __gwt_target
    else
        command gwt "$@"
    fi
}
"#;

const FISH_FUNCTION: &str = r#"
function gwt --wraps gwt --description 'git worktree manager'
    if test (count $argv) -ge 1; and test "$argv[1]" = cd
        # Drop the subcommand, so `gwt cd` with no target still works.
        set -e argv[1]
        set -l __gwt_target (command gwt cd $argv); or return $status
        if test -n "$__gwt_target"
            cd $__gwt_target
        end
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
    fn wrapper_intercepts_cd_only() {
        assert!(POSIX_FUNCTION.contains(r#"if [ "$1" = "cd" ]"#));
        assert!(POSIX_FUNCTION.contains(r#"command gwt "$@""#));
        assert!(FISH_FUNCTION.contains("command gwt $argv"));
    }
}
