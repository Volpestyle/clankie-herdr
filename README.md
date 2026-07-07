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
in this repository alongside the Clankie monorepo so the mux API tracks the
agent and iOS surfaces that consume it. From the product's seat it is simply
Clankie's terminal multiplexer; the fork is how it is maintained, not what it is.

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
  <a href="https://herdr.dev">upstream Herdr</a> · <a href="https://herdr.dev/docs/">runtime docs</a>
</p>

## Role in the system

- **Clankie is the lead agent; `clankie-herdr` is the terminal multiplexer its workers run on.**
  Clankie plans, spawns, watches, unblocks, and harvests; the terminal multiplexer gives each
  worker a real terminal, rolls fleet state up at a glance, and keeps panes alive
  across detach.
- **Product boundary.** Clankie product semantics — orchestration edges,
  transcript policy, pane chat, work tracking, iOS behavior — stay in
  the Clankie repository. This package carries only terminal-multiplexer
  mechanics and the fork-side capabilities those semantics need.

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
changes. The agent-facing mechanics live in the bundled
[`skills/herdr/SKILL.md`](./skills/herdr/SKILL.md)
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
the mux capabilities Clankie needs (request/render serialization, multi-client
retained-render gating, pane/session metadata, and output-change eventing) and
are kept thin so they stay rebasable against upstream. Upstreaming a
broadly-applicable fix stays useful but is no longer required before Clankie can
depend on a fork-carried capability.

## Build

The fork source lives in this package; build the terminal-multiplexer binary from here.

```bash
cargo build --release

just test        # unit tests
just check       # formatting, tests, and maintenance checks
```

The build produces both the upstream-compatible `herdr` binary and Clankie's
`clankie-herdr` binary from the same source. When testing a fresh build from
inside an existing mux session, clear the inherited Herdr socket overrides so
it talks to the debug server — see [`AGENTS.md`](./AGENTS.md) for that and the
full fork-work rules.

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
- [`skills/herdr/SKILL.md`](./skills/herdr/SKILL.md): the reusable agent skill

## Agent instructions

If you are an AI agent working on this package, read [`AGENTS.md`](./AGENTS.md)
before making changes — it is authoritative for fork work — and
[`CONTRIBUTING.md`](./CONTRIBUTING.md) before interacting with the upstream
`ogulcancelik/herdr` repository.

## Upstream and license

`clankie-herdr` tracks [upstream Herdr](https://github.com/herdrdev/herdr) as its
rebase source. Herdr and this fork are licensed under the
[Apache License 2.0](LICENSE).
