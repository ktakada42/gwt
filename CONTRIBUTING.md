# Contributing

Issues and pull requests are welcome — bug reports, ideas for the picker, hook
types you wanted and did not find. Please write them in English so everyone
reading the repository can follow along.

## Getting set up

Requires Rust 1.85+ and Git 2.17+.

```bash
git clone https://github.com/ktakada42/gwx
cd gwx
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
eval "$(gwx shell-init zsh)"
```

## Before opening a pull request

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

Adding a dependency pulls in two more checks, both running whenever
`Cargo.lock` changes. gwx ships prebuilt binaries, so whatever a dependency
brings with it reaches everyone who installs gwx, not only the people who
rebuild from source.

- `cargo audit`, against the [RustSec advisory database](https://rustsec.org),
  and again every Monday — an advisory can appear without anything here
  changing.
- `cargo deny check licenses`, against the policy in `deny.toml`. gwx is MIT,
  so a copyleft dependency arriving in a routine version bump would leave the
  released binaries undistributable.

Both are worth running before you propose a new dependency:

```bash
cargo audit
cargo deny check licenses
```

Everything written into the repository is in English: commits, issues, pull
requests, code, comments and docs. History up to v1.3.2 is in Japanese and
stays that way — tags and releases point at those commits, so rewriting them
would break every link for no real gain.

## Finding your way around

| Path | What lives there |
| --- | --- |
| `src/cli.rs` | The command line, and which completions each argument offers |
| `src/commands/` | One module per subcommand |
| `src/tui.rs` | The interactive picker |
| `src/hooks.rs` | Running `.gwx.toml` hooks |
| `src/git.rs` | Every call out to `git` |
| `src/cd_target.rs` | Handing a directory back to the shell |
| `docs/demo/` | The recordings at the top of the README, and how to remake them |

Two constraints are easy to trip over. The picker draws to `/dev/tty` rather
than stdout, because stdout carries the chosen directory back to the shell
function — anything printed there ends up in a `cd`. And everything the picker
draws is plain ASCII: arrows and box-drawing characters are missing from some
fonts, and characters with an emoji presentation render double width and shift
the columns out of alignment.
