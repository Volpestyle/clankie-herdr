# clankie-herdr — Clankie's terminal multiplexer

<p align="center">
  <img src="assets/logo.png" alt="clankie-herdr" width="100" />
</p>

`clankie-herdr` is Clankie's bundled terminal multiplexer: the terminal runtime that
runs every Clankie worker as a **named, visible, steerable pane** (`clankie:<slug>`)
you can watch go blocked → working → done, attach to, and drive — from the desk
or from the iOS window. It is the component behind Clankie's "visible by design"
promise: no hidden background agents, every worker on one terminal multiplexer that the
always-on brain and the phone see the same way.

Under the hood it is a patch-stack fork of [Herdr](https://herdr.dev), the
terminal-based agent runtime by
[@ogulcancelik](https://github.com/ogulcancelik/herdr). Clankie carries a thin
stack of provider-specific mux patches on top of upstream and vendors the fork
here in the monorepo so the mux API moves on the same commit timeline as the
agent and iOS surfaces that consume it. From the product's seat it is simply
Clankie's terminal multiplexer; the fork is how it is maintained, not what it is.

## Role in the system

- **Clankie is the lead agent; `clankie-herdr` is the terminal multiplexer its workers run on.**
  Clankie plans, spawns, watches, unblocks, and harvests; the terminal multiplexer gives each
  worker a real terminal, rolls fleet state up at a glance, and keeps panes alive
  across detach.
- **Product boundary.** Clankie product semantics — orchestration edges,
  transcript policy, pane chat, work tracking, iOS behavior — stay in
  `clankie-agent` behind the `StageProvider` seam. This package carries only terminal-multiplexer
  mechanics and the fork-side capabilities those semantics need
  ([`clankie-agent` ADR-0017](../clankie-agent/docs/adr/0017-herdr-patch-stack-fork.md)).
- **Decisions.** Fork in the monorepo —
  [ADR-0003](../docs/adr/0003-herdr-fork-in-monorepo.md). Patch-stack fork and
  its boundary —
  [`clankie-agent` ADR-0017](../clankie-agent/docs/adr/0017-herdr-patch-stack-fork.md).
  Shipping the terminal-multiplexer binary version-locked with the brain (**Proposed**) —
  [ADR-0005](../docs/adr/0005-clankie-ships-its-stage.md).

## The terminal multiplexer engine

`clankie-herdr` inherits Herdr's runtime, so everything Herdr gives you is what
Clankie's terminal multiplexer is built on:

- **a real terminal per agent** — each worker's own screen, not an app's
  imitation, so even full-screen TUIs render right.
- **agent state at a glance** — every pane rolls up to 🔴 blocked, 🟡 working,
  🔵 done, or 🟢 idle. Detection works out of the box with process-name matching
  plus terminal-output heuristics; zero config, no hooks required.
- **workspaces, tabs, panes** — organize by repo or folder, click, drag, split;
  mouse-native throughout.
- **nothing dies on detach** — a background server keeps panes and agents alive;
  detach and reattach from any terminal, including the phone over the relay.
- **runs anywhere** — a single ~10MB Rust binary, Linux and macOS (Windows beta),
  no dependencies, inside the terminal you already use.
- **scriptable** — a local Unix socket API and CLI that agents drive to create
  workspaces, split or zoom panes, spawn helpers, read output, and subscribe to
  state changes instead of polling.

## How Clankie drives it

Clankie reaches the terminal multiplexer over that local socket rather than by scraping a screen:
it creates panes, spawns workers, reads their output, and subscribes to state
changes. The agent-facing mechanics live in the bundled [`SKILL.md`](./SKILL.md)
and the [socket API docs](https://herdr.dev/docs/socket-api/). Inside Clankie,
spawns funnel through the agent's transcript-run seam so every worker lands as a
named `clankie:<slug>` pane on Herdr — see `clankie-agent` for that layer.

## Supported workers

Clankie runs coding harnesses as workers on Herdr; detection classifies
each pane's state without per-harness hooks.

| agent | idle / done | working | blocked |
|-------|-------------|---------|---------|
| [pi](https://pi.dev) | ✓ | ✓ | partial |
| [claude code](https://docs.anthropic.com/en/docs/claude-code) | ✓ | ✓ | ✓ |
| [codex](https://github.com/openai/codex) | ✓ | ✓ | ✓ |
| [droid](https://factory.ai) | ✓ | ✓ | ✓ |
| [amp](https://ampcode.com) | ✓ | ✓ | ✓ |
| [opencode](https://github.com/anomalyco/opencode) | ✓ | ✓ | ✓ |
| [grok cli](https://x.ai/grok) | ✓ | ✓ | ✓ |
| [hermes agent](https://github.com/NousResearch/hermes-agent) | ✓ | ✓ | ✓ |
| [kilo code cli](https://kilo.ai/) | ✓ | ✓ | ✓ |
| [devin cli](https://docs.devin.ai/cli) | ✓ | ✓ | ✓ |
| cursor agent | ✓ | ✓ | ✓ |
| antigravity cli | ✓ | ✓ | ✓ |
| kimi code cli | ✓ | ✓ | ✓ |
| [github copilot cli](https://github.com/features/copilot) | ✓ | ✓ | ✓ |
| [qodercli](https://qoder.com/cli) | ✓ | ✓ | ✓ |
| [kiro cli](https://kiro.dev/docs/cli/) | ✓ | ✓ | — |

Any other agent still works; Herdr runs it as a terminal multiplexer, and
custom integrations can report labels and state over the socket API. Detected but
not fully tested: gemini cli, cline. Detection tuning is evidence-based — the
process and hot-reload loop is in [`AGENTS.md`](./AGENTS.md).

## Fork and maintenance

`clankie-herdr` is maintained as a **linear patch stack rebased onto upstream**,
never a merge fork:

- `master` mirrors upstream `ogulcancelik/herdr` and is never committed to
  directly.
- `patch/NN-*` branches each carry one reviewable, stacked patch, in `NN` order.
- `fork` is the stack tip — the branch built, installed, and run as Clankie's
  terminal multiplexer.
- `upstream` is fetch-only.

Rebase, verify, build, install, and push mechanics live in the
`herdr-fork-rebase` host skill; do not reinvent them. Carried patches expose
the `StageProvider` capabilities Clankie needs (request/render serialization,
multi-client retained-render gating, pane/session metadata, watch/presence
eventing) and are kept thin so they stay rebasable against upstream. Upstreaming
a broadly-applicable fix stays useful but is no longer required before Clankie can
depend on a fork-carried capability
([ADR-0017](../clankie-agent/docs/adr/0017-herdr-patch-stack-fork.md)).

## Build

The fork source lives in this package; build the terminal-multiplexer binary from here.

```bash
cargo build --release
./target/release/herdr

just test        # unit tests
just check       # formatting, tests, and maintenance checks
```

The built `herdr` binary is Clankie's terminal multiplexer. When testing a fresh build from
inside an existing mux session, use `cargo run` with the inherited Herdr socket
overrides cleared so it talks to the debug server — see [`AGENTS.md`](./AGENTS.md)
for that and the full fork-work rules.
[ADR-0005](../docs/adr/0005-clankie-ships-its-stage.md) proposes shipping this
binary version-locked and isolated under Clankie's data root; today it coexists
with any user-installed upstream `herdr`.

## Underlying runtime docs

These describe the upstream Herdr runtime this fork builds on and remain the
reference for terminal-multiplexer mechanics:

- [concepts](https://herdr.dev/docs/concepts/): server and client, workspaces,
  tabs, and panes
- [session state](https://herdr.dev/docs/session-state/): detach, restart
  restore, agent restore, and live handoff
- [configuration](https://herdr.dev/docs/configuration/): keybindings, copy mode,
  themes, notifications, environment variables
- [integrations](https://herdr.dev/docs/integrations/): native session restore
  and semantic state per agent
- [socket api](https://herdr.dev/docs/socket-api/): socket protocol and CLI
  reference
- [`SKILL.md`](./SKILL.md): the reusable agent skill

## Agent instructions

If you are an AI agent working on this package, read [`AGENTS.md`](./AGENTS.md)
before making changes — it is authoritative for fork work — and
[`CONTRIBUTING.md`](./CONTRIBUTING.md) before interacting with the upstream
`ogulcancelik/herdr` repository.

## Upstream and license

Herdr is built full-time, in the open, by [@ogulcancelik](https://github.com/ogulcancelik).
`clankie-herdr` tracks it as the rebase source; support upstream development via
[GitHub Sponsors](https://github.com/sponsors/ogulcancelik) (see
[`SPONSORS.md`](./SPONSORS.md)) and reach the maintainer at hey@herdr.dev.

Herdr is dual-licensed, and this fork inherits those terms:

1. Open source: GNU Affero General Public License v3.0 or later
   (AGPL-3.0-or-later).
2. Commercial: commercial licenses are available for organizations that cannot
   comply with AGPL.

Distributing or hosting a build of this fork is AGPL distribution and requires
keeping the license in sync with the published fork; personal or local use adds
no distribution obligation. See
[ADR-0005](../docs/adr/0005-clankie-ships-its-stage.md) for the shipping posture.
