# `gwt remove`: the worktree and, with --with-branch, the branch it was for —
# so finishing a task takes one command too.

Source common.tape

Output remove.gif

# Fits the two listings and the removal in between, with nothing left over.
Set Height 500

Hide
Type "source /demo/env.sh" Enter
Type "bash /demo/seed.sh && clear" Enter
Show

Sleep 1s
Type "gwt list --plain"
Sleep 400ms
Enter
Sleep 2.5s

Type "gwt remove feature/billing --with-branch"
Sleep 800ms
Enter
Sleep 2.5s

Type "gwt list --plain"
Sleep 400ms
Enter
Sleep 3.5s
