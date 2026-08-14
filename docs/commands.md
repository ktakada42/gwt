# Commands

| Command | What it does |
| --- | --- |
| `gwx add <branch>` | Create a worktree for `<branch>`, creating the branch if needed |
| `gwx list` (`ls`) | Pick a worktree interactively; `--plain` or `--paths` for text |
| `gwx cd [<name>]` | Move into a worktree; with no argument, to the main one |
| `gwx remove <name>` (`rm`) | Remove a worktree, optionally with its branch |
| `gwx clean` | Review the worktrees you are done with and remove the ones you pick |
| `gwx init` | Write a `.gwx.toml` template |
| `gwx shell-init <shell>` | Print the `cd` function and the completion hookup |
| `gwx completion <shell>` | Print the completion hookup only |

Every command also has a man page: `man gwx`, `man gwx-add`, and so on.

## How a name is resolved

`<name>` is matched against branch names first, then paths below `base_dir`,
then directory names — so `gwx cd feature/auth` and `gwx cd auth` both work
when they are unambiguous.

A bare `gwx cd` goes to the main worktree, the way a bare `cd` takes you home.
`gwx cd @` says the same thing explicitly. To choose from a list instead, use
[`gwx list`](picker.md).

## `gwx add`

```
gwx add <branch> [--from <commit-ish>] [--path <path>]
                 [--no-create] [--no-hooks] [--force] [--quiet]
```

The branch is resolved in this order:

1. **Local branch exists** → check it out.
2. **Exactly one remote branch matches** → create a local branch tracking it
   (`origin/feature/auth` → `feature/auth`).
3. **Otherwise** → create the branch from `HEAD`, or from `--from` when given.

Use `--no-create` to fail instead of creating a branch, `--path` to override
the generated path, and `--quiet` to print only the resulting path — handy in
scripts:

```bash
cd "$(gwx add feature/auth --quiet)"
```

`--no-hooks` skips the `pre_create` and `post_create` hooks. See
[Hooks](hooks.md).

## `gwx remove`

```
gwx remove <name> [--with-branch] [--force] [--no-hooks]
```

Refuses to delete the main worktree, the worktree you are standing in, or one
with uncommitted changes. `--with-branch` also deletes the branch, but only
when it is merged into `HEAD` of the main worktree; `--force` overrides both
checks. `--no-hooks` skips the `pre_remove` and `post_remove` hooks, which is
the way out when a hook itself is what stands between you and a stale
worktree.

## `gwx clean`

```
gwx clean [--with-branch] [--force] [--no-hooks]
```

Lists every removable worktree with what removing it would cost, ticks the
ones that are merged and clean, and removes what you confirm. See
[`gwx clean`](clean.md) for the states and the keys.

## `gwx list`

Opens [the picker](picker.md) when there is a terminal to draw on and more
than one worktree to choose from. Otherwise it prints a table.

- `--plain` prints the table even in a terminal, for reading or piping.
- `--no-header` leaves out the column labels, which is what a pipe wants.
- `--paths` prints one absolute path per line, and nothing else.

```console
$ gwx list --plain
  WORKTREE         HEAD     PATH
* @                a1b2c3d  /home/me/repo
  feature/auth     a1b2c3d  /home/me/worktrees/feature/auth
```

The table and the picker label their columns the same way. The picker leaves
out `PATH`, which gwx derives from the branch name, and puts a `STATUS` column
there instead.

## `gwx shell-init` and `gwx completion`

`shell-init` prints the shell function that makes `gwx cd` and the picker able
to change your shell's directory, plus the completion hookup. `completion`
prints the completion hookup on its own, for people who do not want the `cd`
function.

Completion is computed by `gwx` itself while you type, so it knows about your
repository:

```console
$ gwx cd <TAB>
@                    -- main worktree — /home/me/repo
feature/auth         -- /home/me/worktrees/feature/auth

$ gwx add <TAB>
hotfix/login         -- local branch
release/2.1          -- origin/release/2.1

$ gwx remove <TAB>   # every worktree except the main one
$ gwx add x --from <TAB>   # branches, tags and remote-tracking branches
```

`gwx add` only offers branches that do not have a worktree yet — the ones it
would actually accept — and remote-only branches under the short name you would
type. Outside a repository nothing is offered instead of an error.
