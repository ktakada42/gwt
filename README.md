# gwt

[![CI](https://github.com/ktakada42/gwt/actions/workflows/ci.yml/badge.svg)](https://github.com/ktakada42/gwt/actions/workflows/ci.yml)
[![Release](https://github.com/ktakada42/gwt/actions/workflows/release.yml/badge.svg)](https://github.com/ktakada42/gwt/actions/workflows/release.yml)
[![codecov](https://codecov.io/gh/ktakada42/gwt/graph/badge.svg)](https://codecov.io/gh/ktakada42/gwt)
[![GitHub release](https://img.shields.io/github/v/release/ktakada42/gwt)](https://github.com/ktakada42/gwt/releases/latest)
[![License](https://img.shields.io/github/license/ktakada42/gwt)](LICENSE)

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
$ gwt list                # pick one: enter to move, ctrl-d to delete
  @             a1b2c3d  /home/me/repo
  feature/auth  a1b2c3d  /home/me/worktrees/feature/auth

$ gwt list --plain        # the same as text, for reading or piping
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
- **List and navigate.** `gwt list` opens an interactive list — move, filter,
  press <kbd>Enter</kbd> to go there or <kbd>Ctrl</kbd>+<kbd>d</kbd> to delete.
  `gwt cd <name>` jumps straight to one, with tab completion.

## Installation

### Homebrew (macOS / Linux)

```bash
brew install ktakada42/tap/gwt
```

To upgrade:

```bash
brew upgrade gwt
```

> [!WARNING]
> Install with the full `ktakada42/tap/gwt` name. Plain `brew install gwt`
> pulls `gwt` from homebrew-core, which is Google Web Toolkit.

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

#### Using oh-my-zsh?

Its `git` plugin defines `alias gwt='git worktree'`, which hides this tool —
zsh expands aliases before it looks for functions or commands. The alias even
swallows the `gwt` inside the snippet above, so sourcing it reports
``unknown subcommand: `shell-init'`` (or `not a git repository` when you are
outside a repo). Drop the alias first:

```zsh
# ~/.zshrc, after `source $ZSH/oh-my-zsh.sh`
unalias gwt 2>/dev/null
eval "$(gwt shell-init zsh)"
```

Order matters: the `unalias` has to come first. If one `.zshrc` is shared
across machines, guard the second line with `command -v gwt >/dev/null &&` so
hosts without gwt stay quiet.

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
| `gwt list` (`ls`) | Pick a worktree interactively; `--plain` or `--paths` for text |
| `gwt cd [<name>]` | Move into a worktree; with no argument, to the main one |
| `gwt remove <name>` (`rm`) | Remove a worktree, optionally with its branch |
| `gwt init` | Write a `.gwt.toml` template |
| `gwt shell-init <shell>` | Print the `cd` function and the completion hookup |
| `gwt completion <shell>` | Print the completion hookup only |

`<name>` is matched against branch names first, then paths below `base_dir`,
then directory names — so `gwt cd feature/auth` and `gwt cd auth` both work
when they are unambiguous.

A bare `gwt cd` goes to the main worktree, the way a bare `cd` takes you home.
`gwt cd @` says the same thing explicitly. To choose from a list instead, use
`gwt list`.

### Picking a worktree interactively

`gwt list` opens a list you can move through:

```
  WORKTREE         HEAD     STATUS
* @                a1b2c3d
  feature/auth     a1b2c3d  dirty, merged
  feature/billing  a1b2c3d  merged
  hotfix           3d3cc2d

> _ type to filter                                        4 worktrees
 up/down  move   enter  cd   ctrl-d   backspace  delete   esc  cancel
```

The table starts at the top, so the header sits against the rows it labels.
The filter joins the help line at the bottom, where the things you operate
live.

The filter line carries a block cursor and, on the right, how much of the list
you are looking at — `1 of 4` once you start typing, so filtering everything
away reads as `0 of 4` rather than an unexplained blank screen.

Each row says what you need before acting on it: `dirty` for uncommitted
changes, `merged` when the branch is already in the main worktree's `HEAD`.
The `STATUS` column fills in a moment after the list appears — working it out
costs a `git status` per worktree, which the list does not wait for.
The path is not shown — gwt derives it from the branch name, so it only
repeated what the first column already said.

Each part of the screen is told apart by a different attribute rather than by
shade alone: the header is bold and underlined, the selected row is highlighted
across the full width, each key in the help line sits in a reverse-video badge,
and only the hints that fade — the placeholder and the count — are dimmed. A
`*` in the first column marks the worktree you are standing in.

| Key | Action |
| --- | --- |
| <kbd>↑</kbd> <kbd>↓</kbd> (or <kbd>Ctrl</kbd>+<kbd>p</kbd> / <kbd>n</kbd>) | Move the cursor |
| type anything | Filter by name or path |
| <kbd>Enter</kbd> | Change into the selected worktree |
| <kbd>Backspace</kbd> | Erase the filter, or remove the worktree once it is empty |
| <kbd>Ctrl</kbd>+<kbd>d</kbd> or <kbd>Delete</kbd> | Remove the worktree, whatever you have typed |
| <kbd>Esc</kbd> or <kbd>Ctrl</kbd>+<kbd>c</kbd> | Leave without moving |

<kbd>Backspace</kbd> does double duty so that the key labelled "delete" on Mac
keyboards — which sends Backspace, not Delete — can remove a worktree. The
bottom line always says which of the two it will do right now. On a narrow
terminal it drops `up/down move` first rather than cutting a word in half.

Holding <kbd>Backspace</kbd> to clear what you typed cannot run past the empty
filter into the delete dialog: the press at that boundary is swallowed, so
reaching the dialog always takes a deliberate keystroke.

Everything the picker draws is plain ASCII, so it does not depend on the font
having arrow or return glyphs.

The confirmation dialog names what is at stake before you answer — uncommitted
changes, and whether the branch is merged:

```
Remove this worktree?

  /home/me/worktrees/feature/auth

  ! uncommitted changes will be lost
  branch `feature/auth` (merged)

[y] remove worktree   [b] remove worktree and branch   [n] cancel
```

The main worktree and the one you are standing in are refused outright, same as
`gwt remove`.

Two cases skip the picker entirely: when there is no terminal to draw on (a
script, a pipe, CI) and when the repository has no worktree other than the main
one. `gwt list` then prints its table, exactly as before, so existing scripts
keep working. Ask for text in a terminal with `gwt list --plain`, or
`gwt list --paths` for one path per line.

The picker hands the directory to the shell function through a temporary file
named by `GWT_CD_FILE`, which leaves stdout free for `gwt list` to print on.
That means `gwt shell-init` changed in v1.1.0: after upgrading, start a new
shell (or re-source your rc file) before `gwt list` can move you.

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

## Contributing

Issues and pull requests are welcome — bug reports, ideas for the picker, hook
types you wanted and did not find. Please write them in English so everyone
reading the repository can follow along.

### Getting set up

Requires Rust 1.82+ and Git 2.17+.

```bash
git clone https://github.com/ktakada42/gwt
cd gwt
cargo test           # unit tests plus end-to-end tests against real repos
```

The end-to-end tests in `tests/cli.rs` create real repositories in a temporary
directory and drive the built binary against them, so they need `git` on your
PATH and nothing else. They are the fastest way to see how a command is
expected to behave.

To try your build without disturbing an installed copy:

```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"   # this shell only
eval "$(gwt shell-init zsh)"
```

### Before opening a pull request

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
```

CI runs the same three on Linux and macOS. Add a test with a behaviour change —
`tests/cli.rs` for anything a user can observe from the command line, unit
tests next to the code for the rest.

Or let a hook run them for you:

```bash
git config core.hooksPath .githooks
```

`.githooks/pre-commit` runs all three before each commit. A commit touching no
Rust — a README fix, say — skips clippy and the tests and returns straight
away, so only the commits that could break the build pay for the wait.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org)
(`feat(list):`, `fix(tui):`, `docs(readme):`). Say why in the body; the diff
already says what.

Everything written into the repository is in English: commits, issues, pull
requests, code, comments and docs. History up to v1.3.2 is in Japanese and
stays that way — tags and releases point at those commits, so rewriting them
would break every link for no real gain.

### Finding your way around

| Path | What lives there |
| --- | --- |
| `src/cli.rs` | The command line, and which completions each argument offers |
| `src/commands/` | One module per subcommand |
| `src/tui.rs` | The interactive picker |
| `src/hooks.rs` | Running `.gwt.toml` hooks |
| `src/git.rs` | Every call out to `git` |
| `src/cd_target.rs` | Handing a directory back to the shell |

Two constraints are easy to trip over. The picker draws to `/dev/tty` rather
than stdout, because stdout carries the chosen directory back to the shell
function — anything printed there ends up in a `cd`. And everything the picker
draws is plain ASCII: arrows and box-drawing characters are missing from some
fonts, and characters with an emoji presentation render double width and shift
the columns out of alignment.

## License

MIT — see [LICENSE](LICENSE).
