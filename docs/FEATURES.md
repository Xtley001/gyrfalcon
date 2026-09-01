# Features and Scope

What `gyrfalcon` v1 is specified to do, what it deliberately does not do, and what is planned next. Written so scope is a decision, not an accident.

> **Status note:** nothing in this repository is a working implementation yet. "Specified" below means the design is complete and consistent across `docs/`, not that code exists. Treat every item as a build target with an acceptance test (mostly the [historical replay harness](./TESTING.md#historical-replay)), not as prior work to wire up.

## Table of Contents

- [v1 — specified](#v1--specified)
- [Roadmap](#roadmap)
- [Non-goals](#non-goals)

## v1 — specified

| Feature | Status | Reference |
|---|---|---|
| Kamino, Save, MarginFi health adapters on one shared framework | Specified | [`ARCHITECTURE.md`](./ARCHITECTURE.md#health-engines) |
| Mint-keyed flash-source router (Kamino/Save as source, all three as target) | Specified | [`ARCHITECTURE.md`](./ARCHITECTURE.md#flash-source-router) |
| Net-EV prioritizer | Specified | [`whitepaper.md`](./whitepaper.md#42-liquidation-payout) |
| LiteSVM simulation with slot-current account sync | Specified | [`ARCHITECTURE.md`](./ARCHITECTURE.md#simulation) |
| ALT-aware, CU-budgeted v0 transaction bundling | Specified | [`ARCHITECTURE.md`](./ARCHITECTURE.md#hard-constraints) |
| Dual submission — staked QUIC + Jito bundle | Specified | [`ARCHITECTURE.md`](./ARCHITECTURE.md#submission-path) |
| Close-factor-based position sizing | Specified | [`STRATEGY.md`](./STRATEGY.md#position-sizing) |
| Per-slot multi-candidate arbitration | Specified | [`STRATEGY.md`](./STRATEGY.md#multi-candidate-arbitration) |
| Contention-driven dynamic tip bidding | Specified | [`STRATEGY.md`](./STRATEGY.md#tip-bidding-model) |
| Treasury exposure caps (per-tx, per-slot, min balance) | Specified | [`STRATEGY.md`](./STRATEGY.md#treasury-and-capital-management) |
| Circuit breakers (revert, drawdown, treasury floor, sync lag) | Specified | [`STRATEGY.md`](./STRATEGY.md#circuit-breakers) |
| Per-protocol fault isolation (one adapter halts, others keep running) | Specified | Invariant I4, [`whitepaper.md`](./whitepaper.md#6-invariants) |
| Historical replay + per-route CU profiling harness | Specified | [`TESTING.md`](./TESTING.md) |
| `observe` / `live` phased deployment modes | Specified | [`RUNBOOK.md`](./RUNBOOK.md#deployment-phases) |
| Charcoal/snow monitoring dashboard | Built — disconnected empty-state UI, zero mock data by design | `../dashboard.html`, [`DATA_POLICY.md`](./DATA_POLICY.md) |

The recommended build sequence for turning this row into working code is [`docs/BUILD_ORDER.md`](./BUILD_ORDER.md), not the table order above.

## Roadmap

Not built yet. Listed so scope creep during v1 development has somewhere else to go.

| Feature | Why it's not in v1 |
|---|---|
| Additional protocols (e.g. new isolated-market lenders) | Adapter framework supports it cheaply once a candidate protocol is chosen, but each still needs its own IDL verification and contention calibration — deliberately sequenced after three protocols are proven in production |
| Multi-region submission | Gated on evidence per [`RUNBOOK.md`](./RUNBOOK.md#deployment-phases) — only justified once data shows races lost specifically to geography |
| Validator-with-stake submission path | Gated on evidence per [`ARCHITECTURE.md`](./ARCHITECTURE.md#submission-path) — only justified once staked-QoS priority is shown to be the binding constraint |
| Automated tip-curve recalibration in `live` mode | v1 calibrates `k` from `observe`-mode data once before promotion (see [`STRATEGY.md`](./STRATEGY.md#cold-start)); continuous online recalibration is a v1.1 target |
| Cross-protocol capital netting (using one protocol's idle collateral as another's flash source) | Adds a shared-capital risk surface the router doesn't currently model; needs its own risk review |
| Public status page / external alerting integrations | Dashboard is currently local-only; hosted version and alert webhooks (PagerDuty/Slack) are planned |

## Non-goals

Explicit, not implicit. These are choices, not gaps.

- **Not an arbitrage bot.** No persistent-spread capture, no cross-DEX arbitrage routing as a strategy in its own right — see [`whitepaper.md §1`](./whitepaper.md#1-motivation) for why the mechanic is fundamentally different.
- **No perpetuals or margin-trading liquidations.** Scope is spot-collateralized lending markets only.
- **No cross-chain coverage.** Solana only; no bridging, no other L1/L2 lending markets.
- **No MEV extraction beyond liquidation capture** — no sandwiching, no generalized backrunning strategy.
- **No discretionary manual trading.** Every submitted transaction is produced by the deterministic pipeline in [`API.md`](./API.md); there is no manual override path that bypasses simulation or treasury caps.
- **No custody of user funds.** The engine liquidates on-chain positions it does not own and never holds borrower assets beyond the atomic flash-borrow/repay window (Invariant I5).
- **No mock or synthetic data on any UI surface, ever.** Not even during early development or for a demo screenshot. See [`DATA_POLICY.md`](./DATA_POLICY.md) — this is a hard rule, not a phased-in one.
