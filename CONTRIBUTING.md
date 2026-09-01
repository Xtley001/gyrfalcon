# Contributing

Thanks for contributing to `gyrfalcon`. This document covers dev setup, the adapter conventions that keep the three protocols consistent, and PR expectations.

If you're implementing this system for the first time, start with [`docs/BUILD_ORDER.md`](./docs/BUILD_ORDER.md), not this file — it defines the sequence and exit criteria for each stage. Don't build Save or MarginFi coverage before the Kamino-only vertical slice (Stage B) passes its exit criteria.

**Hard rule, no exceptions:** no mock, synthetic, hardcoded, or randomly-jittered data on any UI surface, ever — see [`docs/DATA_POLICY.md`](./docs/DATA_POLICY.md). A PR that adds a fabricated value to `dashboard.html` or any future frontend and presents it as live is treated as a policy violation, not a style nit, regardless of how early-stage the surrounding code is.

## Development setup

```bash
git clone https://github.com/Xtley001/gyrfalcon.git
cd gyrfalcon
cargo build --workspace
cargo test --workspace
```

Format and lint before pushing:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

## Adapter conventions

The three protocol adapters share one health-engine framework and implement the `HealthAdapter` trait defined in [`docs/API.md`](./docs/API.md#trait-contracts) — that document is the source of truth for interfaces; update it in the same PR as any signature change. Keep adapters consistent:

- **Decode from the deployed IDL**, not a cached copy. Generate raw Borsh structs; use zero-copy casts where the layout is fixed-size and `#[repr(C)]`-safe.
- **Handle oracle staleness and confidence per protocol.** Never assume one protocol's oracle behavior applies to another — the shared framework must not bake in a single assumption.
- **Isolate faults.** A panic in one adapter must never affect another. Do not share mutable state across adapters.
- **Emit candidates, do not submit.** Adapters compute health and emit breach candidates; routing, prioritization, simulation, and submission are downstream and shared.

## Hot-path rules

- The `strategy → router → simulator → bundler` path is in-process. Do not introduce IPC or async round trips on this path.
- No live aggregator calls on the hot path — use direct, pre-computed AMM routes.
- Any new route must ship with its CU profile and a warmed ALT, verified by the profiling harness.
- Any change to sizing, arbitration, or tip bidding is a change to [`docs/STRATEGY.md`](./docs/STRATEGY.md), not just to `crates/strategy` — update both in the same PR.

## Pull requests

- One logical change per PR. Keep adapter changes separate from framework changes.
- New or changed routes must pass historical replay and CU profiling (see [`docs/TESTING.md`](./docs/TESTING.md)).
- Update the relevant doc in the same PR — README, `docs/`, or `CHANGELOG.md`.
- Follow the [doc style](./docs/) for any markdown: progressive disclosure, tables over prose, fenced code blocks with a language tag.

## Commit style

Use present-tense, imperative subject lines under ~72 characters (`add MarginFi oracle confidence check`). Reference the issue number where one exists.
