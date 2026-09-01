# Gyrfalcon Master Build Order (`BUILD_ORDER.md`)

This document is the authoritative sequence for building and updating `gyrfalcon`. Every step defines its domain context, dependencies, exact code deliverables, and strict acceptance criteria.

---

## Operating Rules (Apply to Every Session Without Exception)

- **R1 — DO NOT INVENT**: If a field, endpoint, parameter, or behavior is not specified, do not create it.
- **R2 — DO NOT ASSUME**: Name any ambiguity before proceeding.
- **R3 — STACK IS LOCKED**: Rust 2021/2024 edition, Tokio multi-threaded runtime, Yellowstone gRPC, Solana SDK, LiteSVM.
- **R4 — NAMES ARE EXACT**: Use exact struct, trait, and field names as specified in [`docs/API.md`](./API.md).
- **R5 — NO UNINVITED CHANGES**: Do not modify files outside the current step's scope.
- **R6 — ONE STEP COMPLETELY**: Fully complete and test the current step before advancing.
- **R7 — TESTABLE DELIVERABLE**: Every step must produce runnable unit tests or verification commands.

---

## Table of Contents

- [Phase 1 — Core Precision & Health Integrity](#phase-1--core-precision--health-integrity)
- [Phase 2 — Multi-Protocol Routing & Token-2022](#phase-2--multi-protocol-routing--token-2022)
- [Phase 3 — Simulation & Historical Replay Harness](#phase-3--simulation--historical-replay-harness)
- [Phase 4 — Live Ingestion & Dual-Path Transports](#phase-4--live-ingestion--dual-path-transports)
- [Phase 5 — Daemon Orchestrator & Live Dashboard](#phase-5--daemon-orchestrator--live-dashboard)

---

## Phase 1 — Core Precision & Health Integrity

### Step 1.1: Base58 Pubkey Integrity
- **Crate:** `gyrfalcon-core` (`crates/core/src/pubkey.rs`)
- **Deliverable:** Update `Pubkey::from_base58` to reject byte lengths $\neq 32$ with `bs58::decode::Error::BufferTooSmall`.
- **Acceptance Criteria:** `cargo test -p gyrfalcon-core pubkey::` passes, confirming short/malformed strings fail decoding.

### Step 1.2: Kamino & Save Close Factor Fix
- **Crate:** `gyrfalcon-health` (`crates/health/src/adapters/kamino.rs`, `save.rs`)
- **Deliverable:** Store `ObligationSnapshot { slot, close_factor_max_repay }` in `known_obligations`. Return real base token repay amount in `HealthAdapter::close_factor()`.
- **Acceptance Criteria:** `cargo test -p gyrfalcon-health kamino::` and `save::` pass, verifying `close_factor()` returns accurate token units.

### Step 1.3: MarginFi Sizing & Liability Calculation
- **Crate:** `gyrfalcon-health` (`crates/health/src/adapters/marginfi.rs`)
- **Deliverable:** Calculate liability token amount from `liability_shares * bank.liability_share_value`. Cache in `AccountSnapshot` and return in `close_factor()`.
- **Acceptance Criteria:** `cargo test -p gyrfalcon-health marginfi::` passes, confirming non-zero `close_factor_max_repay` on breached positions.

### Step 1.4: Circuit Breaker Synchronization
- **Crate:** `gyrfalcon-strategy` (`crates/strategy/src/breakers.rs`)
- **Deliverable:** In `record_success()`, remove route from `routes_taken_out_of_rotation`.
- **Acceptance Criteria:** `cargo test -p gyrfalcon-strategy breakers::` passes all failure-injection tests.

---

## Phase 2 — Multi-Protocol Routing & Token-2022

### Step 2.1: Router Index Optimization & MarginFi Flash Support
- **Crate:** `gyrfalcon-router` (`crates/router/src/lib.rs`)
- **Deliverable:**
  1. Add `observe_marginfi_account` to index native MarginFi flash reserves.
  2. Implement mint-to-reserve index for $O(1)$ lookup per mint.
- **Acceptance Criteria:** `cargo test -p gyrfalcon-router` passes, ranking Kamino, Save, and MarginFi reserves by fee and depth.

### Step 2.2: Token-2022 ATA Pre-Provisioning
- **Crate:** `gyrfalcon-bundler` (`crates/bundler/src/ata.rs`)
- **Deliverable:** Support both SPL Token (`TokenkegQfe...`) and SPL Token-2022 (`TokenzQdBN...`) program IDs in `required_atas` and `create_all_atas_idempotent`.
- **Acceptance Criteria:** Unit tests verify correct ATA derivation for both token programs.

---

## Phase 3 — Simulation & Historical Replay Harness

### Step 3.1: Multi-Protocol Historical Replay Dispatcher
- **Crate:** `gyrfalcon-sim` (`crates/sim/src/bin/replay.rs`)
- **Deliverable:** Dispatch events to `KaminoAdapter`, `SaveAdapter`, and `MarginfiAdapter` based on `event.protocol`.
- **Acceptance Criteria:** `cargo test -p gyrfalcon-sim replay::` passes.

### Step 3.2: LiteSVM Simulation Harness Re-activation
- **Crate:** `gyrfalcon-sim` (`crates/sim/src/lib.rs`, `cu_profile.rs`)
- **Deliverable:** Wire LiteSVM in-process execution into `Simulator::simulate` and re-enable `cu_profile` module.
- **Acceptance Criteria:** Unit tests verify real compute unit measurement against SVM state.

---

## Phase 4 — Live Ingestion & Dual-Path Transports

### Step 4.1: Yellowstone gRPC Live Client
- **Crate:** `gyrfalcon-ingestion` (`crates/ingestion/src/feed.rs`)
- **Deliverable:** Implement `GeyserFeed::connect(url, token)` using `yellowstone-grpc-client` with account filtering and stream heartbeat.
- **Acceptance Criteria:** Connection test passes, streaming account updates into `RingBuffer`.

### Step 4.2: Staked QUIC & Jito Dual Send
- **Crate:** `gyrfalcon-submit` (`crates/submit/src/dual_path.rs`)
- **Deliverable:**
  1. Implement `StakedQuicSendPath` sending raw packets to current/upcoming leader TPU sockets.
  2. Implement `JitoSendPath` sending bundle JSON-RPC requests to Jito Block Engine.
- **Acceptance Criteria:** Unit tests verify race timeout and dual submission behavior.

---

## Phase 5 — Daemon Orchestrator & Live Dashboard

### Step 5.1: Engine Daemon Supervisor
- **Crate:** `gyrfalcon-bin` (`crates/gyrfalcon-bin/src/main.rs`)
- **Deliverable:** Complete multi-threaded pipeline orchestrator:
  - Ingestion thread -> Ring Buffer -> Health Adapters -> Strategy -> Simulation Worker Pool -> Bundler -> Submit -> Store.
  - Signal handling (`SIGINT`, `SIGTERM`) with graceful flush.
- **Acceptance Criteria:** `cargo run --bin gyrfalcon -- --config config/gyrfalcon.example.toml` runs continuously in `observe` mode.

### Step 5.2: Real-Time Dashboard WebSocket Server
- **Crate:** `gyrfalcon-bin` (`dashboard.html` backend)
- **Deliverable:** Embedded async HTTP/WebSocket server in `gyrfalcon-bin` serving `/api/stats` and streaming live liquidation events to `dashboard.html`.
- **Acceptance Criteria:** Opening `dashboard.html` in browser shows `LIVE` status and renders real metrics from running daemon.
