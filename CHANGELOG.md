# Changelog

All notable changes to `gyrfalcon` are documented here. Format follows [Keep a Changelog](https://keepachangelog.com); the project adheres to semantic versioning once a first version actually ships.

> No version has shipped. Everything below is under `[Unreleased]` — this repository is currently a design specification (`docs/`) plus scaffolding, not a working bot. `docs/FEATURES.md` marks each feature "Specified," not "Done," until it passes the acceptance criteria in `docs/BUILD_ORDER.md`. The `[1.0.0]` tag will be cut, with a real date, once the build order's Stage D readiness checklist is complete.

## [Unreleased]

### Added — specification
- Multi-protocol design: Kamino, Save, and MarginFi health adapters on a shared framework (`docs/ARCHITECTURE.md`).
- Mint-keyed flash-source router, independent of the protocol being liquidated (`docs/ARCHITECTURE.md`).
- In-process LiteSVM simulation design with a continuous, slot-current account-sync pipeline (`docs/ARCHITECTURE.md`).
- ALT-aware, CU-budgeted v0 transaction bundling design; dual staked-QUIC + Jito submission path (`docs/ARCHITECTURE.md`).
- Strategy layer: close-factor sizing, per-slot multi-candidate arbitration, contention-driven dynamic tip bidding (`docs/STRATEGY.md`).
- Treasury module design: hot-wallet exposure caps (per-tx, per-slot, minimum balance) and profit sweeps (`docs/STRATEGY.md`).
- Circuit breaker design: consecutive-revert, treasury floor, drawdown, per-protocol sync-lag halt (`docs/STRATEGY.md`).
- Backend interface reference (`docs/API.md`): trait contracts, data model, internal event bus, state store.
- Ordered build sequence (`docs/BUILD_ORDER.md`) from repo scaffold through production readiness.
- Hard no-mock-data policy (`docs/DATA_POLICY.md`) covering the dashboard and all future frontend surfaces.
- Feature scope document (`docs/FEATURES.md`): v1 matrix, roadmap, explicit non-goals.
- Formal profitability model, invariants I1–I6, and worked numeric examples (`docs/whitepaper.md`).
- Historical-replay and per-route CU-profiling harness design (`docs/TESTING.md`); `tests/fixtures/` documented as generated, not pre-supplied.
- Example and devnet configuration files (`config/`) with placeholder risk/treasury values explicitly flagged as such.
- Charcoal/snow monitoring dashboard (`dashboard.html`) — disconnected empty-state UI with zero mock or synthetic data, per `docs/DATA_POLICY.md`.

### Known gaps in the specification (tracked, not yet resolved)
- Key custody at rest for signer and treasury wallets (`docs/SECURITY.md#key-custody`).
- Oracle-manipulation risk, distinct from the already-documented staleness/confidence handling (`docs/whitepaper.md#7-security-considerations`).
- LiteSVM version pinning and vetting process.
- Operating cost model against expected margins (`docs/RUNBOOK.md#operating-cost-model`).
- PnL export path from `store.liquidation_log` to external bookkeeping.
