# Architecture & Technical Decisions (`DECISIONS.md`)

This document is the locked historical log of all architectural and technical decisions made for `gyrfalcon`. Every decision records the context, alternatives considered, the chosen path, and permanent consequences.

---

## D1: Native MarginFi Flash-Loan Integration
- **Date:** 2026-08-30
- **Decision:** MarginFi v2 natively supports flash loans via `lending_account_start_flashloan` and `lending_account_end_flashloan`. The system will treat MarginFi as both a liquidation target and an eligible flash-loan source.
- **Alternatives Considered:**
  1. *Assume MarginFi has no flash loans and always borrow from Kamino/Save* (Initial assumption in `ARCHITECTURE.md`): Disproven by on-chain analysis and security disclosures. Forces unnecessary cross-protocol borrowing when MarginFi itself has deep reserves.
  2. *Support native MarginFi flash loans alongside Kamino and Save* (Chosen).
- **Reason:** Native flash loans avoid cross-program account loading limits and reduce compute unit overhead for MarginFi self-liquidations.
- **Consequences:** `docs/ARCHITECTURE.md`, `docs/BUILD_ORDER.md`, and `crates/router` are updated to support MarginFi as a 3rd flash source.

---

## D2: Correct Close Factor Calculation in Health Adapters
- **Date:** 2026-08-30
- **Decision:** Health adapters must store decoded position state (`slot` and `close_factor_max_repay`) in their internal tracking maps, and `HealthAdapter::close_factor()` must return the actual maximum base-unit repay amount rather than the tracking slot number.
- **Alternatives Considered:**
  1. *Return slot number from `close_factor`*: Critical bug causing multi-million unit sizing errors.
  2. *Re-parse account on every `close_factor` call*: High CPU overhead in the hot arbitration loop.
  3. *Cache `ObligationSnapshot { slot, close_factor_max_repay }` in memory* (Chosen).
- **Reason:** Sizing requires instant $O(1)$ access to the true allowable close-factor repay ceiling without re-running fixed-point arithmetic.
- **Consequences:** `KaminoAdapter`, `SaveAdapter`, and `MarginfiAdapter` store `ObligationSnapshot` / `AccountSnapshot`.

---

## D3: Strict 32-Byte Validation on Base58 Pubkeys
- **Date:** 2026-08-30
- **Decision:** `Pubkey::from_base58` in `crates/core/src/pubkey.rs` must strictly reject any input decoding to $\neq 32$ bytes with `bs58::decode::Error::BufferTooSmall`.
- **Alternatives Considered:**
  1. *Zero-pad short inputs and trust callers*: Severe security vulnerability allowing malformed keys to create bogus PDAs.
  2. *Strict length check `written == 32`* (Chosen).
- **Reason:** Deterministic Solana address integrity across config and runtime decoders.
- **Consequences:** Prevents silent corruption when loading configurations or parsing feeds.

---

## D4: Token-2022 and Transfer-Fee Extension Support
- **Date:** 2026-08-30
- **Decision:** The ATA pre-provisioner and bundler must support both SPL Token (`TokenkegQfe...`) and SPL Token-2022 (`TokenzQdBN...`).
- **Alternatives Considered:**
  1. *Support classic SPL Token only*: Incompatible with modern Solana assets (e.g. PYUSD, tokenized collateral, LSTs).
  2. *Support both programs explicitly* (Chosen).
- **Reason:** Solana lending protocols increasingly list Token-2022 mints.
- **Consequences:** `crates/bundler/src/ata.rs` handles both token program IDs.

---

## D5: Multi-Protocol Historical Replay Dispatcher
- **Date:** 2026-08-30
- **Decision:** The historical replay binary (`crates/sim/src/bin/replay.rs`) dispatches events to `KaminoAdapter`, `SaveAdapter`, or `MarginfiAdapter` based on the event's `protocol` field.
- **Alternatives Considered:**
  1. *Route all events to `KaminoAdapter`*: Causes 100% false detection failures for Save and MarginFi events.
  2. *Dynamic protocol dispatching in replay loop* (Chosen).
- **Reason:** Verifies detection and routing across all three protocols against historical ground truth.
- **Consequences:** `crates/sim/src/bin/replay.rs` supports multi-protocol evaluation.

---

## D6: Asynchronous Non-Blocking Disk Persistence
- **Date:** 2026-08-30
- **Decision:** State store operations (`liquidation_log.append` and `position_book.snapshot`) must use asynchronous background flushing so synchronous disk I/O never blocks the microsecond-sensitive liquidation hot path.
- **Alternatives Considered:**
  1. *Synchronous file writes on every liquidation outcome*: Induces high latency spikes on NVMe/SSD stall.
  2. *Asynchronous background channel writer* (Chosen).
- **Reason:** Latency consistency is critical for racing competing liquidation bots.
- **Consequences:** `crates/store` offloads file writes to background worker threads.

---

## D7: Unified Solana SDK Versioning & Dependency Alignment
- **Date:** 2026-08-30
- **Decision:** Workspace dependencies in `Cargo.toml` are upgraded to modern toolchains (Rust 1.81+, `toml 0.8`, `clap 4.5`, unified `solana-sdk` 1.18+/2.0+), removing artificial Cargo 1.75 legacy constraints.
- **Alternatives Considered:**
  1. *Maintain pinned legacy toml 0.5 and clap 4.5.4*: Incompatible with modern crates like `klend-interface 0.6` and `litesvm`.
  2. *Upgrade workspace toolchain floor* (Chosen).
- **Reason:** Enables compiling real on-chain interface crates and LiteSVM simulation harnesses cleanly.
- **Consequences:** `Cargo.toml` modernized.
