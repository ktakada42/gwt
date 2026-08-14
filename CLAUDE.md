# gwx

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
- Documentation is split on purpose: README is what someone reads once before
  installing, `docs/` is what they come back to. A new flag or hook goes in
  `docs/`; the README grows only when what the tool *is* changes. Its links out
  are absolute GitHub URLs, because crates.io renders the same file and
  relative ones break there.

## Releasing

Maintainer only. Pushing a `v*` tag builds the binaries, creates the GitHub
Release, updates the Homebrew formula, and syncs `Cargo.toml`. The version
string comes from `git describe`, so `Cargo.toml` does not need bumping first.

**The tag annotation is the release notes**, so write it as the Markdown that
should appear on the Releases page, and always tag with `--cleanup=verbatim`:

```bash
git tag -a --cleanup=verbatim -F - v2.1.0 <<'EOF'
v2.1.0

## What changed
...
EOF
```

The subject line is git's, not the reader's: the workflow takes the body from
`%(contents:body)` and names the release after the tag, so whatever stands on
the first line never reaches the Releases page. Keep it to the version — a
subject that repeated the tool's name is what put "gwx v2.1.0" on three release
titles before `--title` was passed explicitly.

Without it, git's default cleanup treats every `##` heading as a comment and
strips it, silently — the message keeps its prose and loses its structure, and
nothing notices until the release is published. v2.1.0 was published that way
and its notes had to be repaired by hand.
