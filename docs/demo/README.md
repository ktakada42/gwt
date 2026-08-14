# The README demos

`add.gif`, `list.gif` and `remove.gif` at the top of the README are recorded
with [vhs](https://github.com/charmbracelet/vhs) from the `.tape` files here.
They are real recordings of the real binary — nothing is staged, so a demo that
looks wrong means the output changed.

Re-record after changing anything they show:

```bash
docs/demo/record.sh            # all three
docs/demo/record.sh list       # just one
```

Docker is the only requirement. `record.sh` builds gwx for Linux in a container
and records in another one, because the paths gwx prints are absolute and a Mac
would put `/private/tmp` or a real user name on screen where the README wants
`/home/me`.

| File | What it is |
| --- | --- |
| `common.tape` | Size, font and theme, sourced by each tape |
| `*.tape` | One recording each, and the keystrokes it plays |
| `setup.sh` | Builds `~/repo`, the throwaway repository being demoed |
| `seed.sh` | Adds the worktrees the `list` and `remove` demos start from |
| `env.sh` | Prompt and shell integration, sourced while vhs is hiding |
| `Dockerfile` | vhs plus git, and a `/home/me` to record in |

The tapes hide their setup and show only the commands, so the recording starts
on a clean prompt. Keep them short — a README GIF is watched once.
