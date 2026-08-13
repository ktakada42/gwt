# `gwt add`: one command turns a branch name into a worktree you are standing
# in, with the hooks from .gwt.toml already run.

Source common.tape

Output add.gif

# Just tall enough for what this demo prints; a half-empty terminal in the
# README is wasted vertical space.
Set Height 470

Hide
Type "source /demo/env.sh" Enter
Show

Sleep 1s
Type "gwt add feature/auth"
Sleep 800ms
Enter
Sleep 3.5s

Type "gwt cd feature/auth"
Sleep 400ms
Enter
Sleep 1.5s

Type "ls -a"
Sleep 400ms
Enter
Sleep 3.5s
