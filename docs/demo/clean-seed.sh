#!/usr/bin/env bash
# A repository at the point where `gwx clean` earns its keep: four worktrees,
# one in each of the states the command tells apart.
#
# It needs a remote, which the other demos do not, so this seeds on its own
# rather than extending seed.sh with something they would all pay for.
set -euo pipefail

git init -q --bare "$HOME/origin.git"
cd "$HOME/repo"
git remote add origin "$HOME/origin.git"
git push -q -u origin main

for branch in feature/auth feature/billing hotfix/login wip/refactor; do
    gwx add "$branch" >/dev/null 2>&1
done

# feature/auth stays as it is: merged into HEAD, nothing uncommitted.

# feature/billing has work of its own, all of it on the remote — a branch in
# review, which is finished for today but not finished with.
billing="$HOME/worktrees/feature/billing"
git -C "$billing" commit -q --allow-empty -m "Send the invoice email"
git -C "$billing" push -q -u origin feature/billing

# hotfix/login has commits that are on no remote at all.
login="$HOME/worktrees/hotfix/login"
git -C "$login" commit -q --allow-empty -m "Fix the login redirect"
git -C "$login" commit -q --allow-empty -m "Add a regression test"

# wip/refactor has edits that were never committed.
echo '// half-finished' >>"$HOME/worktrees/wip/refactor/src/index.js"
