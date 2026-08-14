#!/usr/bin/env bash
# Give the demos a repository that already has worktrees, for the recordings
# that show what a working day looks like rather than the first minute of one.
#
# The states are picked so the picker's STATUS column has something to say:
# one worktree with uncommitted work, one branch that is not merged yet.
set -euo pipefail

for branch in feature/auth feature/billing hotfix/login; do
    gwx add "$branch" >/dev/null 2>&1
done

echo '// TODO: refresh the token' >>"$HOME/worktrees/feature/auth/src/index.js"
git -C "$HOME/worktrees/hotfix/login" commit -q --allow-empty -m "Fix the login redirect"
