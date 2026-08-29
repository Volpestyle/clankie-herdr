---
name: herdr-fork-rebase
description: >-
  Rebase the private herdr patch-stack fork onto upstream, verify, rebuild and
  install the binary, and push the stack to the Volpestyle fork. Use when
  upstream herdr has new commits, when adding/editing a carried patch, or when
  the fork/patch branch layout in ~/dev/herdr needs explaining.
---

# herdr-fork-rebase

Maintains the private herdr fork as a **linear patch stack** rebased onto
upstream. Never merge upstream in; never push to upstream.

## The model

Repo: `~/dev/herdr`. Remotes: `upstream` = `ogulcancelik/herdr` (fetch-only —
its push URL is deliberately set to the invalid `DISABLED_do_not_push_to_upstream`;
leave it that way), `origin` = `Volpestyle/clankie-herdr` (the fork;
force-pushes expected).

- `master` — pristine mirror of `upstream/master`. Never commit to it.
- `patch/NN-<name>` — one branch per carried patch, **stacked** in NN order
  (gaps of 10 for insertions), each ref sitting at its layer's tip commit.
- `fork` — the stack tip. This is what gets built, installed, and run.

`git rerere` is enabled repo-locally with `rerere.autoupdate`, so conflicts
resolved once replay automatically on later rebases. If a *wrong* resolution
gets recorded, purge it with `git rerere forget <path>` before re-resolving.

## The rebase routine

Run `rebase-stack.sh` (in this skill dir), or by hand:

```bash
cd ~/dev/herdr
git fetch upstream
git checkout master && git merge --ff-only upstream/master
git checkout fork   && git rebase --update-refs master
```

`--update-refs` (git ≥ 2.38) moves every reachable `patch/NN` ref during the one
rebase — never rebase patch branches individually. On conflict: resolve (rerere
replays known ones) and run `git rebase --continue` until the rebase completes,
then resume the remaining verification steps or re-run the script.

Then verify → build → install → push:

```bash
cargo check --all-targets
cargo nextest run --locked --status-level fail --final-status-level fail \
  --failure-output final --success-output never
# `just` and `cargo-nextest` come from Homebrew (`brew install just cargo-nextest`);
# they were missing on this machine until 2026-07-03 — plain `cargo test` is the fallback.
cargo build --release
cp ~/.local/bin/herdr ~/.local/bin/herdr.bak
install -m755 target/release/herdr ~/.local/bin/herdr
```

The script then pushes `master`, `fork`, and every local patch ref reachable
from `fork`. Do not replace that selection with a `patch/*` wildcard: this
checkout may contain later, divergent patch work that is not part of the active
stack.

Running herdr servers keep the old binary (old inode) until restarted —
installing is safe while sessions are live; restart to pick up the new build.
If the new client then reports `protocol_mismatch` against that live server,
use `~/.local/bin/herdr.bak` for coordination until the deliberate restart;
never stop the server just to complete this routine.

## Adding / editing a patch

- **Add:** commit on top of `fork`, then `git branch patch/NN-<name>` at it and
  `git branch -f fork` to the new tip. Keep one logical change per patch;
  prefer additive changes (new modules/API methods) over edits woven through
  upstream code — they rebase nearly conflict-free.
- **If `fork` is checked out in the main worktree with another agent's
  uncommitted WIP**, don't touch that tree and don't force-move the ref under
  it. Do the work in a temp worktree (`git worktree add --detach
  /tmp/herdr-<slug> fork` → commit → `git branch patch/NN-<name>`) — without
  `--detach` git refuses because `fork` is already checked out in the main
  worktree. Remove the worktree when done, and advance `fork` later
  (`git branch -f fork patch/NN-<name>`) once the WIP has landed. Until then
  the new patch branch, not `fork`, is the buildable stack tip.
- **Edit:** interactive rebase of the stack with `--update-refs`
  (`git rebase -i --update-refs master` from `fork`), amend the layer commit.
- A patch branch checked out in a worktree blocks `--update-refs` from moving
  it — detach or remove that worktree first (`git worktree list`).

## Conflict gotchas (learned 2026-07-03, first restack)

- Reproduce unexpected integration-test failures in a clean temporary
  worktree at `upstream/master` before changing fork code. If they fail there
  identically, record them as upstream baseline failures and run the rest of
  the suite excluding only those proven failures.
- **Test-file conflicts are almost all protocol-version churn.** Upstream
  replaced hardcoded protocol numbers with a `CURRENT_PROTOCOL` constant;
  carried patches touching tests conflict on exactly that line in dozens of
  places. Resolution: upstream's form wins, always.
- **API-surface patches must regenerate the schema snapshot** or
  `generated_protocol_schema_artifact_is_current` fails:
  `HERDR_UPDATE_API_SCHEMA=1 cargo test generated_protocol_schema_artifact_is_current`
  then commit the updated `docs/next/api/herdr-api.schema.json`.
- **Known integration points for the byte-attach patch** (patch/30): the
  `TerminalRuntime` wrapper in `src/terminal/runtime.rs` must delegate
  `subscribe_output()` to the inner `PaneRuntime`; new upstream
  `ApiRequestMessage` call sites need `stream_to: None`; `PaneRuntime`
  test-struct initializers in `src/pane.rs` need the `output_tx` broadcast
  field; new API types need the `schemars::JsonSchema` derive.
- CHANGELOG conflicts: both sides append bullets under `## Unreleased` —
  keep both, patch bullets after upstream's.

## History note

Pre-stack branches (`fix/foreground-cwd-mcp-subprocess`, `phase2-pane-byte-attach`,
`fix/api-initial-request-read`) and their worktrees under `~/dev/herdr-worktrees/`
are the legacy merge-based integration this stack replaced. Their content lives
in `patch/20`/`patch/30`/`patch/40`. Delete them only with the user's say-so.
