# Error Catalog & Failure Recovery (`ERRORS.md`)

This document is the exhaustive catalog of every error state across the `gyrfalcon` liquidation engine, specifying the trigger condition, error representation, logging behavior, and operational recovery procedure.

---

## 1. Core & Decoder Errors (`gyrfalcon-core`, `gyrfalcon-ingestion`)

### `ERR_PUBKEY_DECODE_FAILED`
- **Trigger:** A base58 string in configuration, command line, or data feeds fails to decode to exactly 32 bytes (or contains non-base58 characters).
- **Type:** `bs58::decode::Error`
- **Logged:** `tracing::error!("failed to decode base58 pubkey: {err}")`
- **Recovery:** Verify the base58 address string. The string must decode to exactly 32 raw bytes.

### `ERR_FEED_DISCONNECTED`
- **Trigger:** The Yellowstone Geyser gRPC stream drops connection, times out, or fails TLS handshake.
- **Type:** `GeyserFeedError::Disconnected(String)`
- **Logged:** `tracing::warn!("geyser gRPC feed disconnected: {reason}, initiating exponential backoff")`
- **Recovery:** Ingestion client triggers reconnect with jittered backoff ($100\text{ms} \to 5\text{s}$). Re-subscribes to watched account owner filters.

### `ERR_RING_BUFFER_FULL`
- **Trigger:** Ingestion pushes accounts faster than health adapters can drain the ring buffer.
- **Type:** `PushError::Full`
- **Logged:** `tracing::error!("ingestion ring buffer full — health engine falling behind by {} slots", lag)`
- **Recovery:** Increments `buffer_full` counter. Sync-lag breaker trips for lagging protocol if slot distance exceeds `risk.sync_lag_halt_slots`.

---

## 2. Health & Protocol Decoding Errors (`gyrfalcon-health`)

### `ERR_ACCOUNT_UNPACK_FAILED`
- **Trigger:** Account data received from Geyser has an unexpected length or discriminator mismatch against pinned protocol structs.
- **Type:** `None` / `DecodeError`
- **Logged:** `tracing::debug!("account {pubkey} failed unpack for protocol {protocol}")`
- **Recovery:** Dropped safely. If persistent across multiple accounts of a program, alerts operator that protocol on-chain layout has upgraded.

### `ERR_ORACLE_PRICE_STALE`
- **Trigger:** The oracle price timestamp in a reserve account is older than `reserve.max_age_price_seconds` or slot age exceeds `STALE_AFTER_SLOTS_ELAPSED`.
- **Type:** `Option<bool>` evaluating to `Some(true)`
- **Logged:** `tracing::warn!("reserve {reserve} oracle price is stale ({age_sec}s > max {max_sec}s) — candidate discarded")`
- **Recovery:** Discards candidate pre-simulation. Clears automatically when a fresh oracle account update is processed.

---

## 3. Strategy & Circuit Breaker Errors (`gyrfalcon-strategy`)

### `ERR_BREAKER_CONSECUTIVE_REVERT`
- **Trigger:** A specific `(protocol, position, flash_reserve)` route reverts `risk.consecutive_revert_limit` times in a row.
- **Type:** `BreakerCheck::Halted(BreakerReason::ConsecutiveRevert)`
- **Logged:** `tracing::error!("circuit breaker TRIPPED: route {route:?} reverted {count} times consecutively — route taken out of rotation")`
- **Recovery:** Scope: Route only. Other routes and protocols continue operating. Re-arm manually via `BreakerState::rearm_route(route)` after triage.

### `ERR_BREAKER_TREASURY_FLOOR`
- **Trigger:** Live hot wallet balance drops below `treasury.min_balance_sol`.
- **Type:** `BreakerCheck::Halted(BreakerReason::TreasuryFloor)`
- **Logged:** `tracing::critical!("circuit breaker TRIPPED: treasury balance {balance} SOL is below minimum {floor} SOL — halting all submissions")`
- **Recovery:** Scope: Whole engine. Stops submitting new candidates. Resets automatically as soon as wallet balance rises above floor.

### `ERR_BREAKER_DRAWDOWN`
- **Trigger:** Rolling 24-hour realized net PnL drops below `-risk.max_drawdown_usd`.
- **Type:** `BreakerCheck::Halted(BreakerReason::Drawdown)`
- **Logged:** `tracing::critical!("circuit breaker TRIPPED: 24h realized drawdown {drawdown_usd} exceeded limit {limit_usd} — full engine halt")`
- **Recovery:** Scope: Whole engine. Requires manual investigation of market conditions and explicit operator re-arm via `BreakerState::rearm_drawdown()`.

### `ERR_BREAKER_SYNC_LAG`
- **Trigger:** An adapter's last processed account slot falls $> \text{risk.sync\_lag\_halt\_slots}$ behind current network slot.
- **Type:** `BreakerCheck::Halted(BreakerReason::SyncLag)`
- **Logged:** `tracing::error!("circuit breaker TRIPPED: protocol {protocol} sync lag ({lag} slots) exceeds limit ({limit}) — adapter halted")`
- **Recovery:** Scope: Protocol only. Other protocols unaffected. Resets automatically when adapter sync catches up.

---

## 4. Simulation & Bundling Errors (`gyrfalcon-sim`, `gyrfalcon-bundler`)

### `ERR_SIMULATION_FAILED`
- **Trigger:** In-process LiteSVM execution reverts (slippage, insufficient flash liquidity, or arithmetic failure).
- **Type:** `ProfileError::SimulationFailed(String)`
- **Logged:** `tracing::debug!("simulation failed for candidate {position}: {reason}")`
- **Recovery:** Discards candidate. No transaction is submitted; zero gas or tip spent.

### `ERR_TX_EXCEEDS_MAX_SIZE`
- **Trigger:** Serialized v0 transaction exceeds Solana's 1232-byte MTU ceiling.
- **Type:** `BundleError::ExceedsMaxSize { actual: usize }`
- **Logged:** `tracing::error!("assembled transaction is {actual} bytes (> 1232 byte ceiling) — route rejected")`
- **Recovery:** Route is discarded. Route requires ALT coverage expansion or instruction compression.

---

## 5. Submission & Network Errors (`gyrfalcon-submit`)

### `ERR_SUBMIT_RACE_LOST`
- **Trigger:** Transaction submitted but reverted because a competing liquidator landed in the block first.
- **Type:** `RevertReason::RaceLost`
- **Logged:** `tracing::info!("liquidation race lost for position {position} at slot {slot}")`
- **Recovery:** Recorded in `liquidation_log`. Strategy updates contention model and evaluates dynamic tip calibration.

### `ERR_SUBMIT_NOT_INCLUDED`
- **Trigger:** Neither Staked QUIC nor Jito bundle resolved before `submit.timeout_ms` elapsed.
- **Type:** `SubmitOutcome::NotIncluded`
- **Logged:** `tracing::warn!("transaction for candidate {position} not included within timeout window")`
- **Recovery:** Evaluates retry eligibility in `strategy::retry_or_drop`.

---

## 6. Configuration & Startup Errors (`gyrfalcon-config`, `gyrfalcon-bin`)

### `ERR_CONFIG_PARSE`
- **Trigger:** TOML syntax error or invalid data types in `config/gyrfalcon.toml`.
- **Type:** `ConfigError::Parse(toml::de::Error)`
- **Logged:** `tracing::error!("failed to parse configuration file: {err}")`
- **Recovery:** Process exits immediately with exit code `1`. Fix TOML syntax.

### `ERR_CONFIG_INVALID_REVERT_LIMIT`
- **Trigger:** `risk.consecutive_revert_limit == 0`.
- **Type:** `ConfigError::InvalidRevertLimit(0)`
- **Logged:** `tracing::error!("consecutive_revert_limit must be >= 1")`
- **Recovery:** Set `consecutive_revert_limit` to $\ge 1$ (recommended: 3).
