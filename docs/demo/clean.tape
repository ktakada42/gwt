# `gwx clean`: four worktrees in four states, and only the finished ones
# ticked. The cursor walks the list so each state can be read before Enter
# removes what was chosen for you.

Source common.tape

Output clean.gif

# The list, the count line, the help line, and the output afterwards.
Set Height 520

Hide
Type "source /demo/env.sh" Enter
Type "bash /demo/clean-seed.sh && clear" Enter
Show

Sleep 1s
Type "gwx clean"
Sleep 500ms
Enter
Sleep 2.5s

Down
Sleep 1.2s
Down
Sleep 1.2s
Down
Sleep 1.5s

Enter
Sleep 3.5s
