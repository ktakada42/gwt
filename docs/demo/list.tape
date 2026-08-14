# `gwx list`: the picker, filtered down to one worktree and entered with
# Enter — the prompt at the end is the whole point.

Source common.tape

Output list.gif

# The picker pins its filter and help line to the bottom row, so a tall
# terminal only buys a gap in the middle of the demo.
Set Height 430

Hide
Type "source /demo/env.sh" Enter
Type "bash /demo/seed.sh && clear" Enter
Show

Sleep 1s
Type "gwx list"
Sleep 500ms
Enter
Sleep 2.5s

Down
Sleep 800ms
Down
Sleep 1.2s

Type "bil"
Sleep 1.8s
Enter
Sleep 3s
