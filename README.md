# gwt

A friendly `git worktree` manager, written in Rust.

`git worktree` is great, but it makes you repeat yourself: you type the branch
name, then a path for it, then you copy over your `.env`, then you reinstall
dependencies, and finally you `cd` into a directory you have to remember.
`gwt` takes care of all of that.

```console
$ gwt add feature/auth
Created branch `feature/auth` from `HEAD`
Running post_create hooks...
  [1/2] copy .env -> .env
  [2/2] npm install
Worktree ready.
/home/me/worktrees/feature/auth

$ gwt cd feature/auth     # with shell integration, this changes directory
$ gwt list
* @             a1b2c3d  /home/me/repo
  feature/auth  a1b2c3d  /home/me/worktrees/feature/auth

$ gwt remove feature/auth --with-branch
```

> [!NOTE]
> `gwt` is inspired by [satococoa/wtp](https://github.com/satococoa/wtp), a Go
> tool with the same goal that is no longer actively maintained. `gwt` is an
> independent reimplementation in Rust — the configuration file and the CLI are
> similar in spirit but not compatible.

## Features

- **One command per branch.** `gwt add <branch>` creates the worktree at a
  predictable path, so you never type a directory name.
- **Branches are created when missing.** An existing local branch is checked
  out, a remote-only branch is tracked, and anything else becomes a new branch.
- **Hooks.** Copy files, create symlinks and run commands before and after the
  worktree is created, configured per repository in `.gwt.toml`.
- **List and navigate.** `gwt list` shows every worktree; `gwt cd <name>` moves
  you into one, with tab completion.

## Installation

### Homebrew (macOS / Linux)

```bash
brew install ktakada42/tap/gwt
```

To upgrade:

```bash
brew upgrade gwt
```

### Shell script (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/ktakada42/gwt/main/install.sh | sh
```

Installs to `~/.local/bin` by default. Override with `INSTALL_DIR`:

```bash
curl -fsSL https://raw.githubusercontent.com/ktakada42/gwt/main/install.sh | INSTALL_DIR=/usr/local/bin sh
```

### cargo install

Requires a [Rust](https://rustup.rs) toolchain.

```bash
cargo install --git https://github.com/ktakada42/gwt
```

> [!WARNING]
> Do not run `cargo install gwt`. The crate name `gwt` on crates.io belongs to
> an unrelated project — this one is packaged as `gwt-rs` and installs a
> command called `gwt`.

### Build from source

```bash
git clone https://github.com/ktakada42/gwt
cd gwt
cargo build --release
./target/release/gwt --help
```

Requires Rust 1.82+ to build and Git 2.17+ at runtime. Linux and macOS are
supported; on Windows the `symlink` hook needs developer mode or elevation.

### Shell integration

A process cannot change the directory of the shell that started it, so `gwt cd`
prints a path and a small shell function does the actual `cd`. The same snippet
registers tab completion for worktree and branch names.

```bash
# ~/.bashrc
eval "$(gwt shell-init bash)"

# ~/.zshrc  (after compinit)
eval "$(gwt shell-init zsh)"

# ~/.config/fish/config.fish
gwt shell-init fish | source
```

Homebrew installs the completion scripts for you, but `gwt cd` still needs the
snippet above.

Without the integration everything still works:

```bash
cd "$(gwt cd feature/auth)"
```

### Completion

Completion is computed by `gwt` itself while you type, so it knows about your
repository:

```console
$ gwt cd <TAB>
@                    -- main worktree — /home/me/repo
feature/auth         -- /home/me/worktrees/feature/auth

$ gwt add <TAB>
hotfix/login         -- local branch
release/2.1          -- origin/release/2.1

$ gwt remove <TAB>   # every worktree except the main one
$ gwt add x --from <TAB>   # branches, tags and remote-tracking branches
```

`gwt add` only offers branches that do not have a worktree yet — the ones it
would actually accept — and remote-only branches under the short name you would
type. Outside a repository nothing is offered instead of an error.

## Commands

| Command | What it does |
| --- | --- |
| `gwt add <branch>` | Create a worktree for `<branch>`, creating the branch if needed |
| `gwt list` (`ls`) | List worktrees; the current one is marked with `*` |
| `gwt cd [<name>]` | Move into a worktree; no argument or `@` means the main worktree |
| `gwt remove <name>` (`rm`) | Remove a worktree, optionally with its branch |
| `gwt init` | Write a `.gwt.toml` template |
| `gwt shell-init <shell>` | Print the `cd` function and the completion hookup |
| `gwt completion <shell>` | Print the completion hookup only |

`<name>` is matched against branch names first, then paths below `base_dir`,
then directory names — so `gwt cd feature/auth` and `gwt cd auth` both work
when they are unambiguous.

### `gwt add`

```
gwt add <branch> [--from <commit-ish>] [--path <path>]
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
cd "$(gwt add feature/auth --quiet)"
```

### `gwt remove`

```
gwt remove <name> [--with-branch] [--force]
```

Refuses to delete the main worktree, the worktree you are standing in, or one
with uncommitted changes. `--with-branch` also deletes the branch, but only
when it is merged into `HEAD` of the main worktree; `--force` overrides both
checks.

## Configuration

`gwt` reads `.gwt.toml` from the **main worktree** — the original clone — no
matter which worktree you run it from, so hook paths always mean the same
thing. Run `gwt init` to get a commented template. Everything is optional.

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

[[hooks.post_create]]
type = "symlink"
from = "node_modules"

[[hooks.post_create]]
type = "command"
command = "npm install"
work_dir = "."         # relative to the new worktree
env = { NODE_ENV = "development" }
```

With `base_dir = "../worktrees"`, a repository at `/home/me/repo` puts the
worktree for `feature/auth` at `/home/me/worktrees/feature/auth`. Slashes in
branch names become directories. An absolute `base_dir` is used as-is.

### Hook types

| Type | Keys | Notes |
| --- | --- | --- |
| `copy` | `from`, `to` | Copies files and directories, including gitignored ones such as `.env` |
| `symlink` | `from`, `to` | Links to the file in the main worktree, for caches you want to share |
| `command` | `command`, `work_dir`, `env` | Run through `/bin/sh -c` |

`from` and `to` are relative paths that may not escape their worktree.

Every hook sees these environment variables:

| Variable | Value |
| --- | --- |
| `GWT_BRANCH` | Branch being checked out |
| `GWT_WORKTREE_NAME` | Name of the worktree |
| `GWT_WORKTREE_PATH` | Absolute path of the new worktree |
| `GWT_MAIN_WORKTREE` | Absolute path of the main worktree |

A failing hook stops the sequence and makes `gwt add` exit non-zero. When a
`post_create` hook fails the worktree has already been created, so it is left
in place for you to inspect. Pass `--no-hooks` to skip them entirely.

## Development

```bash
cargo test           # unit tests plus end-to-end tests against real repos
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## License

MIT — see [LICENSE](LICENSE).
