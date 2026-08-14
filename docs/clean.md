# `gwx clean`

Worktrees pile up. `gwx clean` shows the ones you could remove, says what each
would cost, and removes the ones you tick.

```
Select worktrees to remove

      WORKTREE         STATUS  NOTE
> [x] feature/auth     done    merged into HEAD, nothing uncommitted
  [ ] feature/billing  pushed  not merged; every commit is on its upstream
  [ ] hotfix/login     local   2 commit(s) not on its upstream
  [ ] wip/refactor     dirty   uncommitted changes would be lost

1 of 4 selected
 up/down  move   space  toggle   a  reset   x  none   enter  remove   esc  cancel
```

```
gwx clean [--with-branch] [--force] [--no-hooks]
```

## What the states mean

The classification rests on one fact: **removing a worktree does not remove
commits.** The branch keeps them. So the only thing a removal can destroy is
work that was never committed — everything else here is about whether the work
is *finished*, which is why only `done` is ticked for you.

| State | Meaning | Removing the worktree | Also removing the branch |
| --- | --- | --- | --- |
| `done` | Merged into the main worktree's HEAD, nothing uncommitted | Loses nothing | Loses nothing; the commits are in HEAD |
| `pushed` | Not merged, but every commit is on its upstream | Loses nothing | Loses nothing; the commits are on the remote |
| `local` | Commits that are on no remote, or no upstream at all | Loses nothing; the branch keeps the commits | **Loses those commits** |
| `dirty` | Uncommitted changes | **Loses them** | Same |

A branch whose upstream has been deleted from the remote counts as `local`: the
commits may be on no remote any more, so gwx treats them as yours alone.

## Keys

| Key | Action |
| --- | --- |
| <kbd>↑</kbd> <kbd>↓</kbd> (or <kbd>Ctrl</kbd>+<kbd>p</kbd> / <kbd>n</kbd>) | Move the cursor |
| <kbd>Space</kbd> | Tick or untick the row |
| <kbd>a</kbd> | Back to the starting selection — every `done` row |
| <kbd>x</kbd> | Untick everything |
| <kbd>Enter</kbd> | Remove what is ticked |
| <kbd>Esc</kbd> or <kbd>Ctrl</kbd>+<kbd>c</kbd> | Leave without removing anything |

## Flags

- `--with-branch` deletes each removed worktree's branch too, and only when it
  is merged — the same rule [`gwx remove`](commands.md#gwx-remove) follows.
- `--force` allows removing a `dirty` worktree, and deleting an unmerged
  branch alongside `--with-branch`. Without it, a ticked `dirty` row is
  skipped and said so.
- `--no-hooks` skips `pre_remove` and `post_remove` for the whole run.

Removals happen one at a time and a failure does not stop the rest: you asked
for several removals, not for a transaction. Each one is reported as it goes.

The main worktree and the one you are standing in never appear in the list, the
same way `gwx remove` refuses them.

## Without a terminal

In a script, a pipe or CI there is nobody to tick the boxes, so `gwx clean`
prints the table and removes nothing:

```console
$ gwx clean | cat
feature/auth     done    merged into HEAD, nothing uncommitted
wip/refactor     dirty   uncommitted changes would be lost
gwx clean needs a terminal to choose in; nothing was removed.
```
