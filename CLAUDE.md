# gwt

A `git worktree` manager written in Rust. See README.md for what it does.

## Language

This is a public repository, so **everything that lands in it is written in
English**: commit messages, issue and pull request titles and bodies, code,
comments, documentation, and any string the program prints.

Conversation with the user stays in Japanese, per the global instruction. Only
the artifacts committed or published here switch to English — a review comment
in Japanese is fine, the commit it asks for is not.

Release notes on GitHub Releases are English for the same reason.

History up to v1.3.2 is in Japanese, from before this rule existed. **Leave it
alone.** Tags and releases point at those commits, so rewriting them breaks
every link, and the closed issues are a record of decisions as they were made.
The rule applies from here on, not backwards.

## Commits

Conventional Commits, with a scope where one is obvious:

```
feat(list): open an interactive picker
fix(tui): keep the cursor inside a shrinking list
docs(readme): explain the oh-my-zsh alias conflict
```

Explain **why** in the body, not what the diff already shows. A reader six
months from now needs the reason a trade-off was made, not a summary of the
lines that changed.

## Before committing

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
```

CI runs all three on Linux and macOS, so a failure here is a failure there.

`.githooks/pre-commit` runs them automatically once `core.hooksPath` is set:

```bash
git config core.hooksPath .githooks
```

It skips clippy and the tests for commits that touch no Rust, which keeps a
docs-only commit instant. Do not reach for `--no-verify`; fix the check.

## Working on the code

- Every change is made in a git worktree, never in the main checkout.
- `src/tui.rs` owns the interactive picker; it draws to `/dev/tty` because
  stdout carries the chosen directory back to the shell.
- Anything the picker prints is plain ASCII. Arrows and box-drawing characters
  are missing from some fonts, and characters with an emoji presentation
  render double width and break the column alignment.
- End-to-end tests in `tests/cli.rs` drive the real binary against real
  repositories. Prefer them over mocking git.

## Releasing

Maintainer only. Pushing a `v*` tag builds the binaries, creates the GitHub
Release, updates the Homebrew formula, and syncs `Cargo.toml`. The version
string comes from `git describe`, so `Cargo.toml` does not need bumping first.
