# Session Handoff & Auto-Mode Context (`CONTEXT_HANDOFF.md`)

**Date:** 2026-08-30  
**Project:** `gyrfalcon` — Solana High-Performance Liquidation Engine  
**Status:** Core Bug Fixes Applied (Bugs 1–5), Master Documentation Set Generated, Ready for Ingestion/Transport Wiring & Live Orchestration.

---

## 1. Verified and Working State

- [x] **Core Pubkey Length Strictness (`crates/core/src/pubkey.rs`)**: `Pubkey::from_base58` rejects non-32-byte decodes with `BufferTooSmall`. Unit tested.
- [x] **KaminoAdapter Close Factor Calculation (`crates/health/src/adapters/kamino.rs`)**: `ObligationSnapshot` stores true `close_factor_max_repay`. `close_factor()` returns accurate base token repay ceiling. Unit tested.
- [x] **SaveAdapter Close Factor Calculation (`crates/health/src/adapters/save.rs`)**: `ObligationSnapshot` tracks outstanding debt correctly and returns real repay ceiling. Unit tested.
- [x] **MarginFi Close Factor & Sizing (`crates/health/src/adapters/marginfi.rs`)**: Computes real liability base units from `liability_shares * bank.liability_share_value` instead of `0`. Unit tested.
- [x] **Historical Replay Multi-Protocol Dispatcher (`crates/sim/src/bin/replay.rs`)**: Dispatches events to `KaminoAdapter`, `SaveAdapter`, and `MarginfiAdapter` according to `event.protocol`.
- [x] **Circuit Breaker Route State Synchronization (`crates/strategy/src/breakers.rs`)**: `record_success` unmarks taken-out-of-rotation routes cleanly.
- [x] **Config Schema Completeness (`crates/config/src/lib.rs`)**: Added `min_tip_usd` and `timeout_ms` with serde defaults.
- [x] **Master Specification Documents**:
  - `docs/DECISIONS.md` (Locked technical decisions D1–D7)
  - `docs/ERRORS.md` (Full error catalog across 6 subsystems)
  - `docs/BUILD_ORDER.md` (Strict 5-phase build order with acceptance criteria)
  - `update.md` (Exhaustive line-by-line audit report)

---

## 2. Pause Point / Current State

All Phase 1 foundational bug fixes and documentation consistency checks are applied and verified.

The codebase is ready for:
1. **Yellowstone gRPC Client Integration**: Implementing live stream subscription in `crates/ingestion/src/feed.rs`.
2. **Submission Transports**: Implementing `StakedQuicSendPath` and `JitoSendPath` in `crates/submit/src/dual_path.rs`.
3. **Engine Daemon Pipeline Assembly**: Wiring channels and worker threads in `crates/gyrfalcon-bin/src/main.rs`.
4. **Dashboard Metrics Stream**: Adding WebSocket telemetry server in `crates/gyrfalcon-bin` feeding `dashboard.html`.

---

## 3. Decisions Locked In

- **MarginFi Native Flash Loans**: MarginFi v2 native flash-borrow and flash-repay instructions are supported directly.
- **Token-2022 Compatibility**: Both SPL Token and SPL Token-2022 program IDs are supported in ATA derivation and instruction bundling.
- **Zero-Mock Policy**: Dashboard and state store adhere strictly to `docs/DATA_POLICY.md` (no fake/mock data presented as live).

---

## 4. Next Session Task

**Task:** Complete Phase 4 (Transports & Ingestion) and Phase 5 (Daemon Orchestrator & Dashboard Telemetry).

**Exact Steps:**
1. Wire `yellowstone-grpc-client` into `crates/ingestion/src/feed.rs` for live Geyser streams.
2. Implement live TPU connection and Jito JSON-RPC/gRPC submission in `crates/submit/src/dual_path.rs`.
3. Build the runtime event loop in `crates/gyrfalcon-bin/src/main.rs`.
4. Implement embedded telemetry HTTP/WebSocket server for `dashboard.html`.
