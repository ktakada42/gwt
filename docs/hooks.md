# Hooks

Hooks are declared in `.gwx.toml` and in the user-wide config; see
[Configuration](configuration.md) for the files themselves and how they
combine.

## Phases

| Phase | When | Runs in |
| --- | --- | --- |
| `pre_create` | Before the worktree exists | Main worktree |
| `post_create` | Once the worktree is checked out | New worktree |
| `pre_remove` | Before the worktree is deleted | The worktree being removed |
| `post_remove` | After the worktree — and the branch, with `--with-branch` — is gone | Main worktree |

`post_create` is the only phase with a worktree that is both there and staying,
so it is the only one that takes `copy` and `symlink` hooks. The other three
take `command` hooks, which is what stopping a container or deleting a volume
needs anyway.

The two removal phases run for `gwx remove` and for deleting from the picker
alike — a worktree brought down by <kbd>Ctrl</kbd>+<kbd>d</kbd> is no less
removed. The picker owns the screen while it runs, so it keeps what hooks print
to itself and shows only what a failing one said last.

## Types

| Type | Keys | Notes |
| --- | --- | --- |
| `copy` | `from`, `to` | Copies files and directories, including gitignored ones such as `.env` |
| `symlink` | `from`, `to` | Links to the file in the main worktree, for caches you want to share |
| `command` | `command`, `work_dir`, `env` | Run through `/bin/sh -c` |

`from` and `to` are relative paths that may not escape their worktree.

> [!NOTE]
> **A `copy` keeps symlinks as symlinks.** It recreates them rather than
> following them, dangling ones included — `node_modules/.bin` is full of
> relative links that only work that way, and a removed package leaves behind
> links pointing at nothing.

> [!TIP]
> **On macOS a `copy` is a clone.** APFS shares the blocks between the two
> directories until one of them is written to, and gwx clones the whole tree in
> a single `clonefile` call. A `node_modules` of 10,000 files took **0.13s**,
> against 2.0s for the same copy made file by file — and neither one spends the
> disk space. On Linux a `copy` is a real copy of both the time and the space.

## `copy` or `symlink` for `node_modules`?

Both hooks exist because the answer depends on your platform and on how much
the worktrees should be able to diverge.

| | `copy` | `symlink` |
| --- | --- | --- |
| Disk | Shared blocks on macOS; a real second copy on Linux | Nothing |
| Time | One clone on macOS; file by file on Linux | Instant |
| Installing | Each worktree installs on its own | One directory for all of them — an install in one is an install in every one |

On macOS, `copy` is close to free and leaves the worktrees independent, which
is the reason to prefer it. On Linux, `symlink` is what keeps a large
`node_modules` from being duplicated per worktree — as long as you are not
running installs of different dependency sets side by side.

## Environment

Every hook sees these environment variables:

| Variable | Value |
| --- | --- |
| `GWX_BRANCH` | Branch of the worktree, empty when it has none |
| `GWX_WORKTREE_NAME` | Name of the worktree |
| `GWX_WORKTREE_PATH` | Absolute path of the worktree — in `post_remove`, of the directory that was just deleted |
| `GWX_MAIN_WORKTREE` | Absolute path of the main worktree |

Commands run through `/bin/sh -c` rather than `$SHELL`, so a hook behaves the
same for everyone who clones the repository, and without the `GIT_DIR`-style
variables that would point git at whatever repository invoked gwx.

## When a hook fails

The sequence stops there and the command exits non-zero. **Nothing is rolled
back.** Hooks that already ran stay applied, the rest do not run, and a
`command` hook cannot be undone anyway — there is no un-running `npm install`.
So gwx does not pretend to be atomic; it tells you where it stopped:

```console
$ gwx add feature/auth
Created branch `feature/auth` from `HEAD`
Running post_create hooks...
  [1/3] copy .env -> .env
  [2/3] npm install
        stopped here; 1 of 3 did not run
gwx: the worktree was created and is left at /home/me/worktrees/feature/auth: post_create hook 2/3 failed: npm install: command exited with status 1
```

The numbered list says what ran, the `stopped here` line says what will not,
and the error says which hook and what survives it.

What survives depends on the phase. A `pre_create` or `pre_remove` failure
blocks the operation itself: nothing is created, nothing is deleted. By the
time `post_create` or `post_remove` fails, the worktree has already been
created or removed, and it stays that way — left in place for you to look at
rather than swept away with the evidence.

Removal has one more halfway point: `--with-branch` deletes the worktree first
and the branch second, so a branch that turns out not to be merged leaves the
worktree gone and the branch behind. The error says that too.

Pass `--no-hooks` to `gwx add` or `gwx remove` to skip hooks entirely, which is
also the way out when a hook is what stands between you and a stale worktree.
