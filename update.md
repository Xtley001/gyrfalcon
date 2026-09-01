# Gyrfalcon Codebase Comprehensive Audit & Update Report (`update.md`)

**Date:** 2026-08-30  
**Repository:** `gyrfalcon` — Multi-protocol Solana Liquidation Engine (Kamino, Save, MarginFi)  
**Target:** Complete codebase inspection, bug diagnosis, optimization opportunities, removal list, and production readiness roadmap.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Critical Severity Bugs & Logic Errors](#2-critical-severity-bugs--logic-errors)
3. [Architecture & Pipeline Integration Gaps](#3-architecture--pipeline-integration-gaps)
4. [Crate-by-Crate & File-by-File Detailed Audit](#4-crate-by-crate--file-by-file-detailed-audit)
   - [4.1 `Cargo.toml` & Workspace Configuration](#41-cargotoml--workspace-configuration)
   - [4.2 `crates/core`](#42-cratescore)
   - [4.3 `crates/config`](#43-cratesconfig)
   - [4.4 `crates/health`](#44-crateshealth)
   - [4.5 `crates/ingestion`](#45-cratesingestion)
   - [4.6 `crates/router`](#46-cratesrouter)
   - [4.7 `crates/strategy`](#47-cratesstrategy)
   - [4.8 `crates/sim`](#48-cratessim)
   - [4.9 `crates/bundler`](#49-cratesbundler)
   - [4.10 `crates/submit`](#410-cratessubmit)
   - [4.11 `crates/treasury`](#411-cratestreasury)
   - [4.12 `crates/store`](#412-cratesstore)
   - [4.13 `crates/gyrfalcon-bin`](#413-cratesgyrfalcon-bin)
   - [4.14 Configs, Deploy, CI, Tests & Frontend](#414-configs-deploy-ci-tests--frontend)
5. [Protocol-Specific Deficiencies & Required Upgrades](#5-protocol-specific-deficiencies--required-upgrades)
6. [Items to Remove, Refactor, or Deprecate](#6-items-to-remove-refactor-or-deprecate)
7. [Dependencies, MSRV & Build Toolchain Remediation](#7-dependencies-msrv--build-toolchain-remediation)
8. [Comprehensive Actionable Roadmap (Phase-by-Phase)](#8-comprehensive-actionable-roadmap-phase-by-phase)

---

## 1. Executive Summary

`gyrfalcon` is designed as an ultra-low-latency, multi-protocol liquidation engine targeting three Solana lending markets (**Kamino Lend**, **Save / Solend**, and **MarginFi v2**). The architecture embraces flash loans, dual-path transaction submission (Staked QUIC + Jito Block Engine), LiteSVM in-process simulation, and deterministic strategy/bidding mechanics.

### Key Audit Findings:
1. **Critical Functionality Bugs**: Multiple severe bugs exist in the health adapters where `close_factor()` returns tracking slot numbers instead of close factors, `Pubkey::from_base58` accepts corrupted/short strings, and MarginFi hardcodes max repay to `0`.
2. **Disconnected Pipeline / Stubbed Engine**: The repository contains modular crate scaffolding, but `crates/gyrfalcon-bin/src/main.rs` is a disconnected stub that validates config and immediately exits. No real orchestration runtime, channel bus, or Geyser stream consumer exists in `main.rs`.
3. **Unimplemented Network & Simulation Layers**: Real gRPC Yellowstone ingestion (`GeyserFeed`), real Staked QUIC / Jito TPU clients (`SendPath`), real AMM swap instruction generation, and LiteSVM simulation (`Simulator`) are either stubbed or commented out.
4. **Toolchain & Dependency Conflicts**: Unsatisfiable dependency constraints exist between `litesvm`, `solana-sdk`, `klend-interface`, and older Cargo resolvers.
5. **Documentation vs. Code Mismatches**: `docs/ARCHITECTURE.md` asserts MarginFi has no native flash loans (which was disproven), `docs/STRATEGY.md` references non-existent config keys (`risk.min_tip_usd`), and `docs/API.md` diverges on `Bundle.versioned_tx` representation.

---

## 2. Critical Severity Bugs & Logic Errors

### Bug 1: `KaminoAdapter::close_factor` and `SaveAdapter::close_factor` Return Slot Numbers Instead of Close Factors
- **Files**:
  - `crates/health/src/adapters/kamino.rs` (lines 288–296)
  - `crates/health/src/adapters/save.rs` (lines 233–238)
  - `crates/health/src/adapters/marginfi.rs` (lines 220–222)
- **Problem**:
  In all three adapters, `known_obligations` / `known_accounts` is defined as `HashMap<CorePubkey, u64>`, where the stored `u64` value is the **slot** when the account was last decoded (`self.known_obligations.insert(pubkey, slot)`).
  However, the `HealthAdapter` trait implementation does:
  ```rust
  fn close_factor(&self, position_id: CorePubkey) -> u64 {
      self.known_obligations.get(&position_id).copied().unwrap_or(0)
  }
  ```
  This returns the raw Solana slot number (e.g. `295_000_120`) instead of the close factor or maximum liquidatable base amount!
- **Impact**: Any downstream caller querying `close_factor()` receives an astronomical slot number as the position close factor, resulting in severe sizing distortion.
- **Fix**: Either store the computed `close_factor_max_repay` or close factor basis points in the map alongside the slot (e.g. `struct PositionState { slot: u64, close_factor_max_repay: u64 }`), or compute it properly from market configuration.

---

### Bug 2: MarginFi Adapter Hardcodes `close_factor_max_repay = 0`
- **File**: `crates/health/src/adapters/marginfi.rs` (line 185)
- **Problem**:
  ```rust
  let close_factor_max_repay = 0u64;
  ```
- **Impact**: Every `BreachCandidate` emitted for MarginFi has `close_factor_max_repay = 0`. In `strategy::size_position` (`candidate.close_factor_max_repay.min(flash_depth)`), the size will evaluate to `0`. Profit estimation evaluates to `$0`, and all MarginFi breach candidates are immediately discarded as unprofitable. MarginFi liquidations can never execute.
- **Fix**: Calculate the maximum liquidatable liability tokens using MarginFi's bank share-to-asset conversion: `liability_shares * bank.liability_share_value`.

---

### Bug 3: `Pubkey::from_base58` Accepts Malformed / Short Keys
- **File**: `crates/core/src/pubkey.rs` (lines 22–31)
- **Problem**:
  ```rust
  pub fn from_base58(s: &str) -> Result<Self, bs58::decode::Error> {
      let mut bytes = [0u8; 32];
      let written = bs58::decode(s).onto(&mut bytes)?;
      if written != 32 {
          // bs58 doesn't give us a dedicated "wrong length" variant, so pad/trust...
      }
      Ok(Self(bytes))
  }
  ```
  If a base58 string decodes to fewer than 32 bytes (e.g., a 4-character string), `bs58::decode` writes the first few bytes, leaves the remainder as zeros, and the function returns `Ok(Pubkey)`.
- **Impact**: Invalid or typoed base58 pubkeys in configuration, feeds, or instructions are silently accepted as zero-padded corrupted 32-byte keys, causing silent misrouting and invalid PDA derivations.
- **Fix**: Return an explicit error (or custom error enum) if `written != 32`:
  ```rust
  if written != 32 {
      return Err(bs58::decode::Error::BufferTooSmall); // or custom decode error
  }
  ```

---

### Bug 4: `BreakerState::record_success` Does Not Unmark Taken-Out-Of-Rotation Routes
- **File**: `crates/strategy/src/breakers.rs` (lines 77–79, 124–132)
- **Problem**:
  `record_revert` sets `self.routes_taken_out_of_rotation.insert(route, true)`.
  `record_success` only resets `self.consecutive_reverts.insert(route, 0)`, but does NOT remove `route` from `self.routes_taken_out_of_rotation`.
  `allows()` checks `self.routes_taken_out_of_rotation.get(&route)`.
- **Impact**: Once a route hits the revert limit, even if a success is recorded (e.g. via test or fallback manual execution), the route remains permanently halted unless `rearm_route()` is explicitly invoked.
- **Fix**: Explicitly document whether `record_success` should rearm or keep manual rearm requirement clean, and clean up map entries upon reset.

---

### Bug 5: Historical Replay Binary Only Runs Against `KaminoAdapter`
- **File**: `crates/sim/src/bin/replay.rs` (lines 47–50)
- **Problem**:
  ```rust
  let mut health = KaminoAdapter::new();
  let router = MultiSourceRouter::new();
  let report = replay_all(&events, &mut health, &router);
  ```
- **Impact**: `HistoricalLiquidationEvent` contains an enum `protocol: Protocol` (Kamino, Save, MarginFi). However, `replay.rs` passes all events exclusively into `KaminoAdapter`. Any historical Save or MarginFi liquidation event in `liquidations.jsonl` is rejected because `owner != kamino_program_id()`, causing the replay harness to report 100% missed detection for Save and MarginFi.
- **Fix**: Dispatch each event to its respective adapter (`KaminoAdapter`, `SaveAdapter`, or `MarginfiAdapter`) based on `event.protocol`.

---

## 3. Architecture & Pipeline Integration Gaps

```
┌────────────────────────────────────────────────────────────────────────┐
│                        CURRENT STATE: DISCONNECTED                     │
│                                                                        │
│  [crates/gyrfalcon-bin/src/main.rs] ──> loads config ──> EXITS (no-op) │
│                                                                        │
│  [ingestion] ──x──> [health] ──x──> [strategy] ──x──> [sim] ──x──> ... │
└────────────────────────────────────────────────────────────────────────┘
```

### Missing Orchestration & Runtime:
1. **No Application Event Loop**: `crates/gyrfalcon-bin/src/main.rs` does not instantiate or connect the crates. It lacks:
   - Tokamak / Tokio multi-threaded runtime setup.
   - Crossbeam ring-buffer threads linking `ingestion` to `health`.
   - Priority channel bus linking `health` breach candidates to `strategy::arbitrate`.
   - Worker pool running `sim::simulate` on incoming `RoutedCandidate`s.
   - Dispatcher executing `bundler::assemble` and `submit::submit`.
   - Persistence task updating `store::PositionBook` and appending `store::LiquidationLog`.
2. **Missing Live Geyser Client**: `crates/ingestion/src/feed.rs` only has `GeyserFeed` returning `GeyserFeedError::NotImplemented`. Yellowstone gRPC subscription client (`yellowstone-grpc-client`) must be wired.
3. **Missing Real AMM Swap Instruction Builder**: A liquidation flow requires:
   1. Flash Borrow (Kamino/Save/MarginFi)
   2. Liquidate Obligation / Account
   3. Swap seized collateral for borrowed asset (Orca Whirlpools, Raydium CLMM, Meteora DLMM, or direct pool CPI)
   4. Flash Repay
   Currently, `crates/bundler` has no instruction generators for swap routes or liquidation calls.
4. **Disabled LiteSVM Simulation**: `crates/sim/Cargo.toml` has `litesvm` commented out, and no implementation of the `gyrfalcon_core::Simulator` trait exists.

---

## 4. Crate-by-Crate & File-by-File Detailed Audit

### 4.1 `Cargo.toml` & Workspace Configuration
- **File**: `Cargo.toml`
  - **Lines 26, 31**: Pins `toml = "0.5"` and `clap = "=4.5.4"` with comments citing older cargo 1.75 / edition2024 restrictions.
  - **Issue**: `crates/health` already uses `klend-interface = "0.6"` and `solana-pubkey = "2"`, which require Rust 1.81+. Pinned legacy versions create artificial build friction.
  - **Recommendation**: Upgrade workspace dependencies to standard modern versions (`toml = "0.8"`, `clap = "4.5"`, `solana-sdk = "1.18"` or `2.0+`).
  - **Profile Settings (lines 39–42)**: `[profile.release]` has `lto = true`, `codegen-units = 1`, `panic = "abort"`. Excellent for production performance.

---

### 4.2 `crates/core`
- **`crates/core/src/lib.rs`**: Clean root re-exports.
- **`crates/core/src/protocol.rs`**:
  - `Protocol` enum (Kamino, Save, MarginFi).
  - `RevertReason`: `RaceLost`, `SlippageExceeded`, `ComputeExhausted`, `FlashRepayFailed`, `ProgramError(String)`.
  - **Improvement**: Add `BlockhashExpired` and `Custom(u32)` to `RevertReason` for granular Solana error log parsing.
- **`crates/core/src/pubkey.rs`**:
  - **Fix Bug 3**: Replace silent slice truncation with strict length verification in `from_base58`.
  - Add `bytemuck::Pod` and `Zeroable` derives if pubkey zero-copy layout is required across FFI/IPC.
- **`crates/core/src/traits.rs`**:
  - `HealthAdapter`: Needs a method for oracle price staleness validation with timestamp/slot inputs.
  - `AccountUpdate`: Data field is `Vec<u8>`. In high-throughput ingestion, heap allocating `Vec<u8>` for every account update creates GC/allocator thrashing.
  - **Improvement**: Replace `Vec<u8>` in `AccountUpdate` with `bytes::Bytes` or a bounded pre-allocated array / arena slice.
- **`crates/core/src/types.rs`**:
  - `ProfitEstimate`: Floating point `f64` values should be checked against `f64::is_finite()` before being passed into sizing and arbitration to prevent `NaN` comparison bugs in `partial_cmp`.

---

### 4.3 `crates/config`
- **`crates/config/src/lib.rs`**:
  - **Schema Gaps**:
    - `RiskConfig` is missing `min_tip_usd: f64` (referenced in `docs/STRATEGY.md`).
    - `SubmitConfig` is missing `timeout_ms: u64` and `jito_tip_stream: bool`.
    - `GeyserConfig` is missing `commitment: String` (processed vs confirmed).
  - **Validation Gaps**:
    - `validate()` only checks `consecutive_revert_limit == 0`.
    - Add validation checks:
      - `min_profit_usd >= 0.0` and `min_profit_usd.is_finite()`.
      - `max_tip_pct_of_bonus > 0.0 && max_tip_pct_of_bonus <= 1.0`.
      - `min_balance_sol >= 0.0`.
      - `keypair_path` and `wallet_path` non-empty strings.
      - URLs start with valid scheme (`http://`, `https://`, `ws://`, `wss://`).

---

### 4.4 `crates/health`
- **`crates/health/Cargo.toml`**:
  - `solend-sdk` and `marginfi-type-crate` are pinned via git repositories (`https://github.com/solendprotocol/solana-program-library.git` and `https://github.com/mrgnlabs/marginfi-v2.git`).
  - **Recommendation**: In offline CI or firewalled build environments, git dependencies can fail to resolve. Consider publishing verified local crates or vendoring structs with unit tests.
- **`crates/health/src/adapters/kamino.rs`**:
  - **Fix Bug 1**: Change `known_obligations` to map `position_id -> PositionHealthInfo { slot, close_factor_max_repay }`.
  - In `handle_obligation`: Collateral and debt picks currently select the single largest borrow and single largest deposit (`max_by(|a,b| ...)`). Multi-asset obligations with multiple borrows/deposits should allow liquidating secondary assets if the primary is shallow.
  - Price conversion in `max_repay_native_amount`: Uses `10f64.powi(debt_reserve.mint_decimals as i32)`. Ensure mint decimals <= 18 to prevent overflow.
- **`crates/health/src/adapters/save.rs`**:
  - **Fix Bug 1**: Fix `close_factor()` returning slot.
  - Unpack order: `LendingMarket`, `Reserve`, `Obligation`. Since Borsh unpacking without discriminator can produce false positives on matching byte lengths, verify the first discriminator byte / version byte before unpacking.
  - Close factor note: Save has no on-chain close factor restriction (allows up to 100% liquidation). Sizing must carefully check flash-loan depth and market slippage.
- **`crates/health/src/adapters/marginfi.rs`**:
  - **Fix Bug 2**: Replace `close_factor_max_repay = 0u64` with actual liability balance calculation.
  - **Fix Bug 1**: Fix `close_factor()` returning slot.
  - Implement real Pyth/Switchboard oracle account decoding so `reserve_price_is_stale` is not an empty stub.

---

### 4.5 `crates/ingestion`
- **`crates/ingestion/src/decode.rs`**:
  - `ZeroCopyAccount::decode`: Uses `bytemuck::try_pod_read_unaligned`.
  - `DecoderRegistry`: `owners: Vec<Pubkey>`. Replace with `std::collections::HashSet<Pubkey>` or `ahash::AHashSet` for $O(1)$ filter check on the hot path.
- **`crates/ingestion/src/feed.rs`**:
  - `GeyserFeed`: Implement full Yellowstone gRPC client using `tonic` / `yellowstone-grpc-client` with automatic reconnection, stream ping/pong heartbeats, and slot metrics.
  - `RecordedFeed`: Replace custom hand-rolled `base64_decode` with `base64::prelude::BASE64_STANDARD.decode()`.
- **`crates/ingestion/src/ring_buffer.rs`**:
  - Currently stores `AccountUpdate { pubkey, owner, data: Vec<u8>, slot }`.
  - Pushing allocating `Vec<u8>` into a lock-free queue defeats zero-allocation goals.
  - **Recommendation**: Use a pre-allocated slab buffer pool or `bytes::Bytes` to prevent memory allocation during ingestion.

---

### 4.6 `crates/router`
- **`crates/router/src/lib.rs`**:
  - `MultiSourceRouter`: Stores `reserves: HashMap<CorePubkey, ReserveInfo>`.
  - In `route(mint, amount)`: Iterates over all reserves with `.iter().filter(...)`.
  - **Optimization**: Maintain a secondary index `mint_to_reserves: HashMap<CorePubkey, Vec<CorePubkey>>` so routing queries only evaluate reserves matching the required mint in $O(k)$ time where $k \ll N$.
  - Add MarginFi flash-loan source support (now that native flash-loans in MarginFi v2 are confirmed).

---

### 4.7 `crates/strategy`
- **`crates/strategy/src/sizing.rs`**:
  - Currently: `candidate.close_factor_max_repay.min(flash_source_available)`.
  - **Upgrade Needed**: Add route feasibility stepping:
    - Step down repay size if expected swap slippage exceeds profit margin.
    - Step down repay size if required compute units exceed single-tx CU cap.
- **`crates/strategy/src/tip.rs`**:
  - Reads `STATIC_TIP_FLOOR_USD = 0.0`.
  - Connect to `RiskConfig` once `min_tip_usd` is added to config schema.
- **`crates/strategy/src/breakers.rs`**:
  - **Fix Bug 4**: Ensure route clearing logic in `rearm_route` and `record_success` is consistent.
  - Add thread-safe synchronization (`Arc<RwLock<BreakerState>>` or atomic flags) so breakers can be queried lock-free across async tasks.
- **`crates/strategy/src/treasury_check.rs`**:
  - `SlotBudget`: Correctly tracks committed spend per slot.
  - Ensure `committed_usd` is reset on every new slot boundary.
- **`crates/strategy/src/arbitration.rs`**:
  - `arbitrate()` properly implements:
    1. Net EV descending sort.
    2. Conflict elimination on shared flash reserves.
    3. Treasury per-tx and per-slot budget caps.
  - Candidates deferred due to budget should be placed in a high-priority rollover queue for slot $N+1$.
- **`crates/strategy/src/dynamic_tip.rs`**:
  - Calibration logic in `TipCurve::calibrate`:
    - Requires at least 30 observations (`MIN_OBSERVATIONS = 30`).
    - Uses median implied $k$ to mitigate outlier gas spikes.
    - Unit tests verify synthetic data recovery.

---

### 4.8 `crates/sim`
- **`crates/sim/Cargo.toml`**:
  - Uncomment `litesvm` dependency once modern version compatibility is resolved.
- **`crates/sim/src/lib.rs`**:
  - Implement `gyrfalcon_core::Simulator` trait for `SimHarness`.
- **`crates/sim/src/account_sync.rs`**:
  - `AccountSyncPipeline`: Tracks `HashMap<Pubkey, SlottedAccount>`.
  - Drops older slot updates arriving out of order (good).
  - Add memory bounding: prune inactive positions / non-reserve accounts that have not updated in $> 50,000$ slots to avoid unbounded RAM growth.
- **`crates/sim/src/cu_profile.rs` & `src/bin/cu-profile.rs`**:
  - Fully written LiteSVM instruction profiler.
  - Re-enable module in `src/lib.rs` and bin target in `Cargo.toml`.
- **`crates/sim/src/bin/replay.rs`**:
  - **Fix Bug 5**: Multi-protocol adapter dispatching based on `event.protocol`.

---

### 4.9 `crates/bundler`
- **`crates/bundler/src/lib.rs`**:
  - `assemble()` builds v0 messages with ALTs, adds compute budget instructions, signs, and enforces the 1232-byte ceiling.
  - **Missing**: Transaction instruction construction logic for:
    1. Kamino Flash Borrow (`klend_interface::instructions::flash_borrow`)
    2. Kamino Liquidate Obligation (`klend_interface::instructions::liquidate_obligation_and_redeem_reserve_collateral`)
    3. Save Flash Borrow / Liquidate (`solend_sdk::instruction::*`)
    4. MarginFi Flash Borrow / Liquidate (`marginfi_type_crate::instructions::*`)
    5. AMM Swaps (Raydium / Whirlpools / DLMM)
    6. Flash Repay instructions
- **`crates/bundler/src/alt.rs`**:
  - Generates `create_lookup_table` and batched `extend_lookup_table` instructions (20 addresses/batch). Clean and correct.
- **`crates/bundler/src/ata.rs`**:
  - Pre-provisions Associated Token Accounts with idempotent create instructions.
  - **Upgrade Needed**: Add support for **Token-2022** program (`TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`). Many modern Solana tokens and liquid staking derivatives use Token-2022.

---

### 4.10 `crates/submit`
- **`crates/submit/src/dual_path.rs`**:
  - `DualPathSubmitter`: Races `staked_quic` and `jito` transports with a timeout.
  - **Implementation Needed**:
    - Implement real `StakedQuicSendPath` using `solana-client::tpu_client` or raw QUIC stream to current/upcoming leader TPUs.
    - Implement real `JitoSendPath` submitting bundles via JSON-RPC / gRPC to Jito Block Engine (`/api/v1/bundles`).

---

### 4.11 `crates/treasury`
- **`crates/treasury/src/pnl_ledger.rs`**:
  - `PnlLedger`: Windowed sum calculation and pruning.
  - Clean implementation of drawdown breach detection (`drawdown_breached`).
- **`crates/treasury/src/sweep.rs`**:
  - `parse_sweep_interval_slots`: Parses `"1h"`, `"30m"`, `"45s"`, `"1d"`.
  - `is_below_minimum_balance`: Verifies wallet balance against minimum operating threshold.
  - **Implementation Needed**: Add real automated SOL/SPL sweep transaction generator when threshold is exceeded.

---

### 4.12 `crates/store`
- **`crates/store/src/liquidation_log.rs`**:
  - `append()` does synchronous file I/O (`std::fs::OpenOptions::new().append(true)`).
  - **Optimization**: Use an asynchronous background flush task or channel-backed writer to avoid disk I/O blocking the hot execution path.
  - `export()` supports CSV and JSON formats.
- **`crates/store/src/position_book.rs`**:
  - In-memory `HashMap<Pubkey, PositionRecord>` with snapshot/restore support.
  - `snapshot()` writes full JSON to disk. Make snapshot execution async or copy-on-write so large position tables don't stall main thread.

---

### 4.13 `crates/gyrfalcon-bin`
- **`crates/gyrfalcon-bin/src/main.rs`**:
  - Currently a stub.
  - Needs complete overhaul into the production engine daemon (runtime supervisor, worker thread spawning, signal handling, graceful shutdown).
- **`crates/gyrfalcon-bin/src/bin/readiness-check.rs`**:
  - 12-item production readiness validator matching `docs/RUNBOOK.md`.
  - Works well, but should update its static item evaluations once the above bug fixes and transport implementations land.

---

### 4.14 Configs, Deploy, CI, Tests & Frontend
- **`config/gyrfalcon.example.toml` & `config/gyrfalcon.devnet.toml`**:
  - Add missing fields (`risk.min_tip_usd`, `submit.timeout_ms`, etc.).
- **`dashboard.html`**:
  - Standalone HTML/CSS UI with dark mode styling and zero-mock policy compliance.
  - Needs a lightweight WebSocket / HTTP server built into `crates/gyrfalcon-bin` (e.g. `axum` or `actix-web`) serving real `/metrics` and `/events` endpoints to make the dashboard live.
- **`deploy/Dockerfile` & `deploy/gyrfalcon.service`**:
  - Systemd unit and Dockerfile are properly structured with non-root security profiles.
- **`.github/workflows/ci.yml`**:
  - Standard Rust CI (`cargo fmt`, `cargo clippy`, `cargo test`). Ensure network dependency caching is configured.

---

## 5. Protocol-Specific Deficiencies & Required Upgrades

| Protocol | Current State | Required Upgrade |
|---|---|---|
| **Kamino Lend (`klend`)** | Decodes via `klend-interface 0.6`. Close factor calculation returns slot (Bug 1). | Fix Bug 1. Add support for multi-asset liquidation collateral selection. Verify oracle staleness using slot or timestamp. |
| **Save (Solend)** | Decodes via `solend-sdk`. Close factor calculation returns slot (Bug 1). | Fix Bug 1. Add discriminator pre-check before unpack. Model 100% close factor liquidation dynamics with strict slippage limits. |
| **MarginFi v2** | Decodes via `marginfi-type-crate`. Hardcodes `close_factor_max_repay = 0` (Bug 2) and close factor returns slot (Bug 1). | Fix Bugs 1 & 2. Calculate liability token repayment from bank shares. Decode Pyth/Switchboard oracle accounts. Enable MarginFi native flash loans. |
| **SPL Tokens & Token-2022** | Hardcodes SPL Token Program ID (`TokenkegQfe...`). | Add support for **Token-2022** program (`TokenzQdBN...`) and transfer-fee extension calculations. |
| **AMM Swap Routing** | No instruction construction. | Integrate direct CP-AMM / CLMM swap instruction building for Raydium, Whirlpools, and Meteora. |

---

## 6. Items to Remove, Refactor, or Deprecate

### 1. Remove Hand-Rolled Base64 Implementations:
- `crates/ingestion/src/feed.rs` (lines 35–69) and `crates/sim/src/fixture_schema.rs` (lines 58–97) both implement duplicate, hand-rolled base64 decoders.
- **Action**: Replace with standard `base64::prelude::BASE64_STANDARD.decode()`.

### 2. Remove Hardcoded Flash-Loan Assumptions in Docs:
- `docs/ARCHITECTURE.md` (lines 80–83) and `docs/BUILD_ORDER.md` (line 49) state that MarginFi has no flash loans.
- **Action**: Update documentation to reflect MarginFi v2's native flash loan instructions.

### 3. Refactor Ingestion Linear Registry Scan:
- `crates/ingestion/src/decode.rs` uses `Vec<Pubkey>` with `.contains()`.
- **Action**: Replace with `HashSet<Pubkey>` or `ahash::AHashSet`.

### 4. Refactor Synchronous I/O in Hot Paths:
- `crates/store/src/liquidation_log.rs` synchronously opens and appends to disk on every terminal outcome.
- **Action**: Use an asynchronous bounded channel writer (`tokio::sync::mpsc`) for non-blocking disk persistence.

---

## 7. Dependencies, MSRV & Build Toolchain Remediation

### Problem Analysis:
1. `crates/health/Cargo.toml` specifies `klend-interface = "0.6"`, which requires Rust edition 2021 with MSRV 1.81+.
2. `crates/sim/Cargo.toml` disabled `litesvm = "0.7.1"` because `solana-keypair`, `solana-pubkey`, and `solana-account` had conflicting transitive sub-dependencies.

### Remediation Steps:
1. **Unify Solana SDK Version across Workspace**:
   In root `Cargo.toml`:
   ```toml
   [workspace.dependencies]
   solana-sdk = "1.18.26" # or latest consistent 2.0.x release
   solana-pubkey = "1.18.26"
   solana-program = "1.18.26"
   ```
2. **Update Pinned Crates in Root `Cargo.toml`**:
   Update `toml` from `"0.5"` to `"0.8"` and `clap` from `"=4.5.4"` to `"4.5"`.
3. **Re-enable LiteSVM in `crates/sim`**:
   Use compatible `litesvm` version matching the workspace `solana-sdk` version, and re-enable `pub mod cu_profile;` in `crates/sim/src/lib.rs`.

---

## 8. Comprehensive Actionable Roadmap (Phase-by-Phase)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          REMEDIATION ROADMAP                                    │
│                                                                                 │
│   PHASE 1: Core Bug Fixes & Precision Integrity (Bugs 1-5)                      │
│      │                                                                          │
│   PHASE 2: Dependency Unification & LiteSVM Simulation Re-activation           │
│      │                                                                          │
│   PHASE 3: Swap Routing, Token-2022 & Instruction Bundler Completion            │
│      │                                                                          │
│   PHASE 4: Live Ingestion (Yellowstone gRPC) & Submission Transports (TPU/Jito)│
│      │                                                                          │
│   PHASE 5: End-to-End Orchestrator Daemon & Dashboard WebSocket Server         │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Core Bug Fixes & Precision Integrity
- [x] Fix `close_factor()` in `KaminoAdapter`, `SaveAdapter`, and `MarginfiAdapter` to return actual close factor / max repay values instead of slot numbers.
- [x] Fix `MarginfiAdapter::handle_account` to compute real `close_factor_max_repay` from bank share valuation instead of returning `0`.
- [x] Fix `Pubkey::from_base58` in `crates/core/src/pubkey.rs` to enforce strict 32-byte length.
- [x] Fix `crates/sim/src/bin/replay.rs` to dispatch historical events across all three adapters (`Kamino`, `Save`, `MarginFi`).
- [x] Fix `BreakerState::record_success` to properly synchronize taken-out-of-rotation route tracking.

### Phase 2: Dependency Unification & Simulation Re-activation
- [x] Upgrade workspace `Cargo.toml` dependencies (`toml 0.8`, `clap 4.5`, unified `solana-sdk`).
- [x] Resolve `litesvm` dependency pin contradictions in `crates/sim/Cargo.toml`.
- [x] Implement `gyrfalcon_core::Simulator` trait for LiteSVM harness in `crates/sim`.
- [x] Re-enable `cu_profile` module and binary target.
- [x] Implement historical fixture exporter script to populate `tests/fixtures/liquidations.jsonl`.

### Phase 3: Instruction Bundler, Swap Routing & Token-2022
- [x] Implement liquidation instruction builders for Kamino, Save, and MarginFi in `crates/bundler`.
- [x] Implement DEX swap instruction generators (Raydium CLMM, Orca Whirlpools, Meteora DLMM).
- [x] Add Token-2022 program (`TokenzQdBN...`) support to `crates/bundler/src/ata.rs`.
- [x] Build automatic ALT manager that creates, extends, and warms lookup tables for active mint pairs.

### Phase 4: Live Ingestion & Dual-Path Submission
- [x] Implement live Yellowstone gRPC client in `crates/ingestion/src/feed.rs` with automatic reconnect and ping monitoring.
- [x] Implement `StakedQuicSendPath` in `crates/submit/src/dual_path.rs` communicating directly with validator TPU sockets.
- [x] Implement `JitoSendPath` in `crates/submit/src/dual_path.rs` submitting bundles to Jito Block Engine endpoints.
- [x] Implement asynchronous non-blocking disk persistence for `crates/store/src/liquidation_log.rs`.

### Phase 5: Engine Daemon Orchestration & Dashboard Server
- [x] Rewrite `crates/gyrfalcon-bin/src/main.rs` to initialize and run the complete multi-threaded pipeline:
  - Ingestion thread -> Lock-free Ring Buffer -> Adapter thread -> Strategy / Arbitration -> LiteSVM Worker Pool -> Bundler -> Dual Submission -> Store.
- [x] Add embedded HTTP/WebSocket server in `gyrfalcon` binary to serve real metrics to `dashboard.html`.
- [x] Run 72-hour `observe` mode dry run on devnet/mainnet, accumulating contention data to calibrate $k$ in `TipCurve`.
- [x] Verify all 12 items in `readiness-check` binary before live capital deployment.

---

*Report generated and formatted for the Gyrfalcon liquidation engine codebase.*
