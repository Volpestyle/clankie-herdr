#!/usr/bin/env bash
# Rebase the herdr patch-stack fork onto upstream, verify, build, install, push.
# Idempotent: re-run after resolving a rebase conflict; completed steps no-op.
# Env toggles: SKIP_TESTS=1  SKIP_INSTALL=1  SKIP_PUSH=1
set -euo pipefail

REPO="${HERDR_REPO:-$HOME/dev/herdr}"
BIN="$HOME/.local/bin/herdr"
cd "$REPO"

if [ -n "$(git status --porcelain)" ]; then
  echo "error: working tree dirty — commit/stash first" >&2
  exit 1
fi
if [ -d "$(git rev-parse --git-path rebase-merge)" ] || [ -d "$(git rev-parse --git-path rebase-apply)" ]; then
  echo "error: rebase in progress — resolve and 'git rebase --continue' first" >&2
  exit 1
fi

echo "==> fetch upstream"
git fetch upstream

echo "==> fast-forward master to upstream/master"
git checkout -q master
git merge --ff-only upstream/master

echo "==> rebase patch stack (fork + patch/* refs move together)"
git checkout -q fork
git rebase --update-refs master

echo "==> verify"
cargo check --all-targets
if [ -z "${SKIP_TESTS:-}" ]; then
  cargo nextest run --locked --status-level fail --final-status-level fail \
    --failure-output final --success-output never
fi

echo "==> build release"
cargo build --release

if [ -z "${SKIP_INSTALL:-}" ]; then
  echo "==> install to $BIN (backup at $BIN.bak; running servers keep old inode until restart)"
  [ -f "$BIN" ] && cp "$BIN" "$BIN.bak"
  install -m755 target/release/herdr "$BIN"
fi

if [ -z "${SKIP_PUSH:-}" ]; then
  echo "==> push stack to fork (origin)"
  git push --force-with-lease origin master fork 'refs/heads/patch/*:refs/heads/patch/*'
fi

echo "==> done"
git log --oneline upstream/master..fork
