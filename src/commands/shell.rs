//! `gwt shell-init` / `gwt completion` — shell integration.

use anyhow::Result;
use clap::CommandFactory;

use crate::cli::{Cli, Shell, ShellArgs};

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

pub fn completion(args: ShellArgs) -> Result<()> {
    let mut cmd = Cli::command();
    let mut out = Vec::new();
    clap_complete::generate(
        clap_complete::Shell::from(args.shell),
        &mut cmd,
        "gwt",
        &mut out,
    );
    print!("{}", String::from_utf8_lossy(&out));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated(shell: Shell) -> String {
        let mut cmd = Cli::command();
        let mut out = Vec::new();
        clap_complete::generate(clap_complete::Shell::from(shell), &mut cmd, "gwt", &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn completions_are_generated_for_every_shell() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let script = generated(shell);
            assert!(script.contains("gwt"), "empty completion for {shell:?}");
        }
    }

    #[test]
    fn wrapper_intercepts_cd_only() {
        assert!(POSIX_FUNCTION.contains(r#"if [ "$1" = "cd" ]"#));
        assert!(POSIX_FUNCTION.contains(r#"command gwt "$@""#));
        assert!(FISH_FUNCTION.contains("command gwt $argv"));
    }
}
