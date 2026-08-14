# Configuration

`gwx` reads `.gwx.toml` from the **main worktree** — the original clone — no
matter which worktree you run it from, so hook paths always mean the same
thing. Run `gwx init` to get a commented template. Everything is optional.

There is a second file, in the same format, for what is yours rather than the
project's: `~/.config/gwx/config.toml` (or `$XDG_CONFIG_HOME/gwx/config.toml`).
See [Two files](#two-files) for how the two combine.

```toml
version = "1"

[defaults]
# Where worktrees are created, relative to the main worktree.
base_dir = "../worktrees"

# Runs before the worktree exists, in the main worktree. Commands only.
[[hooks.pre_create]]
type = "command"
command = "git fetch --prune"

# Runs inside the new worktree, in order.
[[hooks.post_create]]
type = "copy"
from = ".env"          # relative to the main worktree
to = ".env"            # relative to the new worktree; defaults to `from`

# Share one node_modules with the main worktree. On macOS a `copy` is a
# clone and costs about the same — see hooks.md for which to pick.
[[hooks.post_create]]
type = "symlink"
from = "node_modules"

[[hooks.post_create]]
type = "command"
command = "npm install"
work_dir = "."         # relative to the new worktree
env = { NODE_ENV = "development" }

# Runs inside the worktree while it is still there. Commands only.
# A failure calls the removal off.
[[hooks.pre_remove]]
type = "command"
command = "docker compose down"

# Runs once the worktree is gone, in the main worktree. Commands only.
[[hooks.post_remove]]
type = "command"
command = "rm -rf \"$GWX_MAIN_WORKTREE/.cache/$GWX_WORKTREE_NAME\""
```

What the hooks themselves can do is in [Hooks](hooks.md).

## `base_dir`

With `base_dir = "../worktrees"`, a repository at `/home/me/repo` puts the
worktree for `feature/auth` at `/home/me/worktrees/feature/auth`. Slashes in
branch names become directories. An absolute `base_dir` is used as-is.

Two things are expanded before the path is used:

| | |
| --- | --- |
| `~` | Your home directory, at the start of the path only (`~/worktrees`) |
| `{repo}` | The directory name of the main worktree |

Both exist for the user-wide config, where one line has to work in every
repository:

```toml
# ~/.config/gwx/config.toml
[defaults]
base_dir = "~/worktrees/{repo}"
```

That puts this repository's `feature/auth` at `~/worktrees/gwx/feature/auth`,
and the next repository's somewhere it cannot collide with. gwx never runs the
value through a shell, so anything else in braces, and `~user` for somebody
else's home, is an error rather than a directory with a surprising name.

## Two files

`.gwx.toml` in the repository is a decision the project makes: everyone who
clones it gets the same hooks, and it is meant to be committed. Where you like
your worktrees is not that kind of decision, and neither is what you personally
run in every checkout — those go in `~/.config/gwx/config.toml`, which nobody
else sees.

```toml
# ~/.config/gwx/config.toml
[defaults]
base_dir = ".worktrees"        # inside the repository, wherever you are
# or: "~/worktrees/{repo}"     # all of them together, one per repository
```

The two combine per key rather than one replacing the other:

| | Result |
| --- | --- |
| `[defaults]` | The repository wins where it says something; where it is silent, yours applies |
| Hooks | Both run. Neither replaces the other |

Hooks run in the order setting up and tearing down always take: on **create**,
yours first and the repository's second; on **remove**, the repository's first
and yours last — so what your hook set up is still there while the project's
teardown runs. `gwx` names each hook as it runs it, so the order is visible.

Two things to know before putting hooks in the user-wide file. They run in
**every** repository, including ones you have just cloned and have not read;
and a `copy` hook naming a file that a repository does not have is a failure,
not a skip. `--no-hooks` on `add` and `remove` is the way past a hook that is
in your way.
