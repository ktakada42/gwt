#!/usr/bin/env bash
# Build the throwaway repository the README demos are recorded against.
#
# It lives in $HOME so the recordings show `~/repo` and `/home/me/worktrees/…`
# rather than the maintainer's own paths. The .tape files call this while the
# recording is hidden; run it by hand to try a demo out.
set -euo pipefail

repo="$HOME/repo"

rm -rf "$repo" "$HOME/worktrees"
mkdir -p "$repo"
cd "$repo"

git init -q -b main
git config user.name "Dev"
git config user.email "dev@example.com"
git config commit.gpgsign false

mkdir -p src node_modules
printf 'DATABASE_URL=postgres://localhost/app\n' >.env
printf '{\n  "name": "app",\n  "version": "1.0.0"\n}\n' >package.json
printf 'node_modules\n.env\n' >.gitignore
printf 'console.log("hello");\n' >src/index.js

cat >.gwt.toml <<'EOF'
version = "1"

[[hooks.post_create]]
type = "copy"
from = ".env"

[[hooks.post_create]]
type = "symlink"
from = "node_modules"
EOF

git add -A
git commit -qm "Initial commit"

# Branches for the demos to check out. `feature/auth` is deliberately missing:
# `gwt add` creates it on camera, which is the interesting half of the command.
git branch feature/billing
git branch hotfix/login
