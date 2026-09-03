# Comprehensive Audit: 50 Bugs, Trade Blockers, and Unprofitability Flaws in Gyrfalcon

This document details **50 critical bugs, trade execution blockers, unprofitability flaws, mathematical errors, and architectural breakdowns** identified across the Gyrfalcon codebase (`crates/*`, `config/*`, and `dashboard.html`).

---

## Table of Contents
1. [Core Pipeline & Execution Blockers (Why No Trades Can Land)](#1-core-pipeline--execution-blockers-why-no-trades-can-land) (Bugs 1–10)
2. [Instruction Building & Discriminator Errors (On-Chain Reverts)](#2-instruction-building--discriminator-errors-on-chain-reverts) (Bugs 11–20)
3. [DEX Routing & Swap Execution Flaws (Zero Slippage Protection & Missing Venues)](#3-dex-routing--swap-execution-flaws-zero-slippage-protection--missing-venues) (Bugs 21–28)
4. [Flash Loan Routing & Gating Bugs (Accounting, Tag & Layout Errors)](#4-flash-loan-routing--gating-bugs-accounting-tag--layout-errors) (Bugs 29–34)
5. [Health Factor & Breach Detection Inaccuracies (Passive State & Stale Oracles)](#5-health-factor--breach-detection-inaccuracies-passive-state--stale-oracles) (Bugs 35–40)
6. [Strategy, Sizing & Arbitration Flaws (EV Destruction & Unprofitability)](#6-strategy-sizing--arbitration-flaws-ev-destruction--unprofitability) (Bugs 41–44)
7. [Submission, Latency & Jito Auction Flaws (Wasted Tips & False Confirms)](#7-submission-latency--jito-auction-flaws-wasted-tips--false-confirms) (Bugs 45–48)
8. [Treasury, Ledger & Dashboard Infrastructure (Capital Leaks & State Drift)](#8-treasury-ledger--dashboard-infrastructure-capital-leaks--state-drift) (Bugs 49–50)

---

## 1. Core Pipeline & Execution Blockers (Why No Trades Can Land)

### 1. Empty Dummy Transaction Buffer Submitted by Daemon
- **File**: [`crates/gyrfalcon-bin/src/main.rs:255`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L255)
- **Description**: The main loop constructs a bundle with `versioned_tx: vec![0u8; 64]`. The daemon never invokes `gyrfalcon_bundler::assemble()` to compile instructions into a real Solana v0 transaction.
- **Impact**: The engine submits 64 zero-bytes to validator TPU and Jito Block Engine sockets. Every transaction is rejected immediately by the SVM deserializer; zero trades can ever be submitted or landed.

### 2. Signer Keypair is Never Loaded in the Execution Daemon
- **File**: [`crates/gyrfalcon-bin/src/main.rs`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs)
- **Description**: While `Config` defines `identity.keypair_path`, `main.rs` never reads this file or instantiates a `solana_sdk::signature::Keypair`.
- **Impact**: Without a private key, the system cannot sign transactions. Even if transaction instructions were constructed, they cannot be authorized or submitted to the network.

### 3. Yellowstone Geyser Client Background Task Hangs Indefinitely
- **File**: [`crates/ingestion/src/feed.rs:143-156`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/ingestion/src/feed.rs#L143-L156)
- **Description**: `GeyserFeed::connect()` spawns a background task with an empty loop that merely logs `"Connecting..."` and sleeps with backoff. It never opens a gRPC stream and never sends an `AccountUpdate` through `sender`.
- **Impact**: `geyser_feed.next_account().await` in `main.rs:187` awaits on the mpsc receiver forever. The main event loop never receives a single account update; breach detection never runs.

### 4. Hardcoded Dummy Geyser URL Causes Immediate Exit
- **File**: [`crates/gyrfalcon-bin/src/main.rs:172-178`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L172-L178)
- **Description**: `main.rs` hardcodes `geyser_url = "https://solana-yellowstone-grpc.example.com"` instead of reading `config.geyser.url`. If connection fails, it logs a warning and exits with `ExitCode::SUCCESS`.
- **Impact**: The engine cannot connect to a real Yellowstone endpoint and immediately shuts down without attempting any trades.

### 5. Main Binary Excluded from Root Cargo Workspace
- **File**: [`Cargo.toml:3-11`](file:///c:/Users/pc/Desktop/gyrfalcon/Cargo.toml#L3-L11)
- **Description**: The workspace `members` list contains only 7 crates (`core`, `config`, `strategy`, `bundler`, `store`, `router`, `submit`). `crates/gyrfalcon-bin` is excluded.
- **Impact**: Running `cargo build --workspace` or `cargo build --bin gyrfalcon` fails because `gyrfalcon-bin` is an unlinked workspace package.

### 6. Strategy Crate Missing `size_and_route` Function
- **File**: [`crates/gyrfalcon-bin/src/main.rs:25`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L25), [`crates/strategy/src/lib.rs`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/strategy/src/lib.rs)
- **Description**: `main.rs` attempts to import `use gyrfalcon_strategy::sizing::size_and_route;`, but `size_and_route` does not exist in `crates/strategy`.
- **Impact**: Compilation error prevents `crates/gyrfalcon-bin` from compiling.

### 7. Signature Mismatch in Circuit Breaker Method Calls
- **File**: [`crates/gyrfalcon-bin/src/main.rs:279-283`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L279-L283), [`crates/strategy/src/breakers.rs:69-77`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/strategy/src/breakers.rs#L69-L77)
- **Description**: `main.rs` calls `breaker_state.record_success(routed.candidate.protocol, actual_net_usd)` and `record_revert(routed.candidate.protocol, reason)`. However, `record_success` takes `(RouteKey)`, and `record_revert` takes `(RouteKey, u32)`.
- **Impact**: Type mismatch prevents compilation of `crates/gyrfalcon-bin`.

### 8. Hardcoded Localhost RPC Endpoint for Staked QUIC Send
- **File**: [`crates/gyrfalcon-bin/src/main.rs:167`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L167)
- **Description**: `main.rs` hardcodes `StakedQuicSendPath::new("http://127.0.0.1:8899")` instead of reading `config.staked_send.url`.
- **Impact**: On production servers, all transactions submitted over the staked send path fail with connection refused to `127.0.0.1:8899`.

### 9. Hardcoded Global Jito Endpoint Bypasses Regional Routing
- **File**: [`crates/gyrfalcon-bin/src/main.rs:168`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L168)
- **Description**: `main.rs` hardcodes `https://mainnet.block-engine.jito.wtf` instead of reading `config.jito.block_engine_url` (which specifies Frankfurt in `config/gyrfalcon.toml`).
- **Impact**: Submissions incur cross-continental RTT latency, guaranteeing lost bundle races to colocated competitors.

### 10. Five Core Crates Excluded from Workspace to Mask Build Failures
- **File**: [`Cargo.toml:3-11`](file:///c:/Users/pc/Desktop/gyrfalcon/Cargo.toml#L3-L11)
- **Description**: `crates/health`, `crates/sim`, `crates/treasury`, `crates/ingestion`, and `crates/gyrfalcon-bin` are omitted from the root workspace `members`.
- **Impact**: The workspace gives a false impression of "green tests" (82 passing tests) while the actual daemon, ingestion, simulation, and health adapter layers are uncompiled and untested.

---

## 2. Instruction Building & Discriminator Errors (On-Chain Reverts)

### 11. Kamino Liquidation Instruction Uses an Incorrect Discriminator
- **File**: [`crates/bundler/src/liquidations.rs:59`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L59)
- **Description**: `build_kamino_liquidate_instruction` hardcodes discriminator `[0xb8, 0xb8, 0x48, 0x4f, 0x86, 0x3a, 0x8d, 0xd0]`. The true Anchor discriminator for `liquidate_obligation_and_redeem_reserve_collateral` (`sha256("global:liquidate_obligation_and_redeem_reserve_collateral")[..8]`) is `[0xb1, 0x47, 0x9a, 0xbc, 0xe2, 0x85, 0x4a, 0x37]`.
- **Impact**: The Kamino program fails instruction dispatch and reverts on-chain.

### 12. Kamino Liquidation Instruction Omits Lending Market Authority and Oracles
- **File**: [`crates/bundler/src/liquidations.rs:63-75`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L63-L75)
- **Description**: Kamino's `LiquidateObligationAndRedeemReserveCollateral` instruction expects `lending_market_authority` PDA, reserve oracle accounts, and sysvar instructions. `build_kamino_liquidate_instruction` only passes 11 accounts, missing these required accounts.
- **Impact**: The Kamino program reverts with `AccountNotEnoughKeys` or invalid PDA verification.

### 13. MarginFi Liquidation Instruction Uses an Incorrect Discriminator
- **File**: [`crates/bundler/src/liquidations.rs:151`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L151)
- **Description**: `build_marginfi_liquidate_instruction` hardcodes discriminator `[0xea, 0xef, 0x1d, 0x05, 0x16, 0x6a, 0x82, 0x88]`. The true Anchor discriminator for `lending_account_liquidate` (`sha256("global:lending_account_liquidate")[..8]`) is `[0xd6, 0xa9, 0x97, 0xd5, 0xfb, 0xa7, 0x56, 0xdb]`.
- **Impact**: MarginFi v2 rejects the instruction with `InstructionFallbackNotFound`.

### 14. MarginFi Liquidation Omits Oracle Accounts in Remaining Accounts
- **File**: [`crates/bundler/src/liquidations.rs:154-167`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L154-L167)
- **Description**: MarginFi requires Pyth/Switchboard oracle accounts for all active banks in `remaining_accounts` to assess health at liquidation time. None are passed.
- **Impact**: MarginFi v2 reverts with missing oracle or stale oracle errors.

### 15. Save (Solend) Liquidation Instruction Omits Required Program Accounts
- **File**: [`crates/bundler/src/liquidations.rs:108-120`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L108-L120)
- **Description**: Save instruction 13 (`LiquidateObligationAndRedeemReserveCollateral`) requires 15 accounts including `destination_liquidity`, `withdraw_reserve_liquidity_supply`, `withdraw_reserve_collateral_mint`, and `sysvar_instructions`. Only 11 are provided.
- **Impact**: Save on-chain processor fails with `InstructionUnpackError` or missing account keys.

### 16. Generic Flash Loan Borrow Uses Solend 1-Byte Tag on Kamino
- **File**: [`crates/bundler/src/liquidations.rs:186`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L186)
- **Description**: `build_flash_borrow_instruction` passes a single byte `14u8`. Kamino Lend is an Anchor program requiring an 8-byte discriminator (`flash_borrow_reserve_liquidity`).
- **Impact**: Kamino rejects the flash loan borrow as an invalid instruction.

### 17. Generic Flash Loan Repay Uses Solend 1-Byte Tag on Kamino
- **File**: [`crates/bundler/src/liquidations.rs:215`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L215)
- **Description**: `build_flash_repay_instruction` passes a single byte `15u8` instead of Kamino's 8-byte Anchor repay discriminator.
- **Impact**: Kamino rejects the flash loan repay instruction, causing transaction revert.

### 18. MarginFi Flash Loan Instructions Never Borrow or Transfer Capital
- **File**: [`crates/bundler/src/liquidations.rs:303-347`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/liquidations.rs#L303-L347)
- **Description**: MarginFi's `start_flashloan` and `end_flashloan` instructions only flag `ACCOUNT_IN_FLASHLOAN`. They do not transfer tokens to the liquidator. Capital must be borrowed via `lending_account_borrow`. No borrow instruction is built.
- **Impact**: Liquidator wallet receives 0 flash loan funds, and the subsequent liquidation instruction reverts due to insufficient balance.

### 19. Phoenix Swap Instruction Encoded With Invalid Custom Binary Format
- **File**: [`crates/bundler/src/swaps.rs:226-233`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/swaps.rs#L226-L233)
- **Description**: `build_phoenix_swap_instruction` writes `[0u8, side, amount_in, min_out]` (18 bytes). Phoenix expects a Borsh-serialized `OrderPacket` struct containing tick prices, lot counts, and time validity flags (>40 bytes).
- **Impact**: Phoenix fails Borsh deserialization and aborts execution.

### 20. Phoenix Swap Instruction Omits Trader Seat Account
- **File**: [`crates/bundler/src/swaps.rs:235-245`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/swaps.rs#L235-L245)
- **Description**: Phoenix markets enforce trader Seat accounts PDA. `build_phoenix_swap_instruction` omits the Seat account.
- **Impact**: Markets requiring seats reject the swap transaction.

---

## 3. DEX Routing & Swap Execution Flaws (Zero Slippage Protection & Missing Venues)

### 21. Raydium CLMM Swaps Omit Required Tick Arrays
- **File**: [`crates/bundler/src/swaps.rs:71-81`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/swaps.rs#L71-L81)
- **Description**: Raydium CLMM swaps require `amm_config` and initialized tick array accounts in `remaining_accounts` to traverse ticks during swaps. `build_raydium_clmm_swap_instruction` passes 0 tick arrays.
- **Impact**: Raydium CLMM program halts on missing tick accounts.

### 22. Meteora DLMM Swaps Omit Bin Arrays
- **File**: [`crates/bundler/src/swaps.rs:172-184`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/swaps.rs#L172-L184)
- **Description**: Meteora DLMM swaps require bin array accounts in `remaining_accounts` to traverse active liquidity bins. None are provided.
- **Impact**: Meteora DLMM swaps fail on-chain.

### 23. Missing Raydium CP Swap Builder
- **File**: [`crates/bundler/src/swaps.rs:16-20`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/swaps.rs#L16-L20)
- **Description**: `raydium_cp_swap_program_id()` is defined, but no builder function (`build_raydium_cp_swap_instruction`) exists.
- **Impact**: When `DexRouter` selects Raydium CP Swap, the bundler cannot generate instructions.

### 24. Missing Jupiter Aggregator v6 Fallback Instruction Builder
- **File**: [`crates/bundler/src/swaps.rs:40-44`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/swaps.rs#L40-L44)
- **Description**: `jupiter_program_id()` is defined, but no instruction builder exists for Jupiter swaps.
- **Impact**: Routes requiring Jupiter fallback cannot be compiled.

### 25. `DexRouter` is Never Wired into the Engine
- **File**: [`crates/router/src/dex.rs`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/router/src/dex.rs), [`crates/gyrfalcon-bin/src/main.rs:161`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L161)
- **Description**: `DexRouter` is never instantiated or called in `main.rs`, `strategy`, or `sim`. `main.rs` only instantiates `MultiSourceRouter`.
- **Impact**: Seized collateral swap routing is entirely unexecuted.

### 26. `DexRouter` Quote Table is Never Populated
- **File**: [`crates/router/src/dex.rs:64, 75-85`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/router/src/dex.rs#L64)
- **Description**: `DexRouter::markets` is an in-memory map that is only updated via `register_market_quote()`, which is never called outside of unit tests.
- **Impact**: `select_route()` always returns `None`, making swap routing impossible.

### 27. Token-2022 Transfer Fees Ignored During Repay Sizing
- **File**: [`crates/bundler/src/ata.rs:34-42`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/ata.rs#L34-L42)
- **Description**: `calculate_token_2022_transfer_fee` is defined but never called in sizing or flash repay calculations.
- **Impact**: Liquidating Token-2022 tokens with transfer fees leaves the wallet short on repay tokens, causing the flash loan repay instruction to revert.

### 28. Single Token Program Parameter Assumed for Multi-Token Routes
- **File**: [`crates/bundler/src/ata.rs:50-63`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/bundler/src/ata.rs#L50-L63)
- **Description**: `required_atas` accepts a single `token_program` parameter applied to all mints. If a route pairs an SPL Token with a Token-2022 mint, the ATA for one of them is derived against the wrong program.
- **Impact**: Invalid ATA PDA derivation causes transaction failure.

---

## 4. Flash Loan Routing & Gating Bugs (Accounting, Tag & Layout Errors)

### 29. MarginFi Bank Parser Treats Group Pubkey as Mint
- **File**: [`crates/router/src/lib.rs:196-199`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/router/src/lib.rs#L196-L199)
- **Description**: `observe_marginfi_account` reads `data[8..40]` as the token mint. In MarginFi v2 `Bank`, offset 8..40 is `group: Pubkey`, while `mint` is at offset 40..72.
- **Impact**: The router indexes MarginFi banks under group keys instead of mint keys; flash loan queries for debt mints never match.

### 30. MarginFi Available Liquidity Parsed from Mint Bytes
- **File**: [`crates/router/src/lib.rs:202-206`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/router/src/lib.rs#L202-L206)
- **Description**: Reads `data[40..48]` as `available_liquidity` (`u64`). That offset corresponds to the first 8 bytes of the `mint` pubkey.
- **Impact**: Arbitrary public key bytes are interpreted as token balance, producing bogus liquidity figures.

### 31. Save (Solend) Repay Sizing Underflows by 6–9 Orders of Magnitude
- **File**: [`crates/health/src/adapters/save.rs:197`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/save.rs#L197)
- **Description**: Computes `close_factor_max_repay = decimal_to_f64(liquidity.borrowed_amount_wads) as u64`. `decimal_to_f64` divides by 1e18, converting wads to whole token units (e.g. 100 USDC -> 100). But `BreachCandidate` expects base units (e.g. 100_000_000 micro-USDC).
- **Impact**: The engine liquidates 100 micro-units ($0.0001), burning transaction fees and tips for zero profit.

### 32. Fabricated Flash Loan Verification Signatures in `DECISIONS.md`
- **File**: [`DECISIONS.md:52, 69`](file:///c:/Users/pc/Desktop/gyrfalcon/DECISIONS.md#L52)
- **Description**: `DECISIONS.md` cites `5wL9QkQZ3dM4k6yU8Vp1c3...` and `3xN7fLqK9YvP2...` as evidence of verified flash loans.
- **Impact**: These are truncated dummy strings, violating the gating requirement in `01_PROTOCOLS.md §2`.

### 33. Unnecessary Flash Reserve Locking Starves Candidate Execution
- **File**: [`crates/strategy/src/arbitration.rs:73-79, 93`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/strategy/src/arbitration.rs#L73-L79)
- **Description**: `arbitrate` locks the flash reserve per slot, deferring any subsequent candidate using the same reserve. Because flash loans are atomic within each transaction, independent transactions can borrow from the same reserve in the same slot.
- **Impact**: The engine artificially limits itself to 1 liquidation per reserve per slot, letting competing searchers take the remaining liquidations.

### 34. Hardcoded Flash Depth of 50 Billion Base Units in Main Loop
- **File**: [`crates/gyrfalcon-bin/src/main.rs:239`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L239)
- **Description**: `let flash_depth = 50_000_000_000;` is hardcoded regardless of token mint decimals or actual pool depth.
- **Impact**: Sizes positions to 50,000 USDC or 50 SOL without verifying that the reserve holds that balance, causing flash loans to fail on smaller pools.

---

## 5. Health Factor & Breach Detection Inaccuracies (Passive State & Stale Oracles)

### 35. Passive On-Chain Obligation Health Misses Market Price Crashes
- **File**: [`crates/health/src/adapters/kamino.rs:28-41`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/kamino.rs#L28-L41)
- **Description**: Kamino obligations only update risk metrics when `refresh_obligation` is called. When market prices collapse, the on-chain account data remains unchanged until a competitor calls `refresh_obligation`.
- **Impact**: Gyrfalcon only detects breaches after a competing bot has already initiated liquidation, arriving 100% late.

### 36. MarginFi Health Cache Does Not Update on Price Drops
- **File**: [`crates/health/src/adapters/marginfi.rs:37-66, 147-156`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/marginfi.rs#L37-L66)
- **Description**: `MarginfiAccount.health_cache` is only refreshed on active user operations (borrow/withdraw). Price changes do not update the cache.
- **Impact**: Underwater MarginFi accounts still display `HEALTHY = true`, blinding Gyrfalcon to breach events.

### 37. Zero-Collateral Positions Cause 100% Loss of Repaid Debt
- **File**: [`crates/health/src/adapters/kamino.rs:200-205`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/kamino.rs#L200-L205), [`crates/health/src/adapters/marginfi.rs:164`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/marginfi.rs#L164)
- **Description**: When `unhealthy == 0.0` or `asset_value == 0.0`, `health_factor == 0.0 < 1.0`. The position is flagged as breached.
- **Impact**: The bot repays debt on an account with zero collateral, seizing 0 tokens and losing 100% of the repaid capital.

### 38. Arbitrary Collateral Selection Without DEX Swap Market Check
- **File**: [`crates/health/src/adapters/kamino.rs:219-224`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/kamino.rs#L219-L224), [`crates/health/src/adapters/save.rs:188-193`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/save.rs#L188-L193)
- **Description**: The adapter picks the collateral deposit with the largest market value without verifying if a DEX liquidity pool exists to swap that asset back to the debt asset.
- **Impact**: Liquidating illiquid collateral causes the swap leg to fail, reverting the entire flash loan bundle.

### 39. Redundant Deserialization of Save Accounts in Ingestion Loop
- **File**: [`crates/health/src/adapters/save.rs:236-244`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/save.rs#L236-L244)
- **Description**: Calls `Reserve::unpack_from_slice()` and `Obligation::unpack_from_slice()` twice sequentially for every account update.
- **Impact**: Adds unnecessary CPU deserialization latency in the critical path.

### 40. MarginFi Liquidation Sizing Ignores Allowed Close Factor
- **File**: [`crates/health/src/adapters/marginfi.rs:192`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/health/src/adapters/marginfi.rs#L192)
- **Description**: MarginFi enforces a maximum liquidation close factor (typically 50%), but `MarginfiAdapter` sets `close_factor_max_repay = liability_native` (100%).
- **Impact**: MarginFi's program reverts on-chain with `LiquidationExceedsCloseFactor`.

---

## 6. Strategy, Sizing & Arbitration Flaws (EV Destruction & Unprofitability)

### 41. Feasibility Stepping Cannot Reduce Serialized Transaction Size
- **File**: [`crates/strategy/src/sizing.rs:245-308`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/strategy/src/sizing.rs#L245-L308)
- **Description**: If a transaction exceeds `MAX_TX_BYTES` (1232 bytes), stepping down `repay_amount` does not alter instruction count or account count. The serialized byte length remains identical.
- **Impact**: The stepping loop decrements 10 times and reduces `final_size` to 0, dropping all large-account candidates.

### 42. Static Tip Bidding Produces Non-Competitive $0.25 Bids
- **File**: [`crates/strategy/src/tip.rs:38-41`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/strategy/src/tip.rs#L38-L41)
- **Description**: `static_tip_bid` clamps `risk.min_tip_usd` ($0.25) against `ceiling`. It does not scale dynamically with the liquidation bonus.
- **Impact**: Bidding $0.25 on contested liquidations with $500+ bonuses guarantees 100% loss against competing MEV searchers bidding 40–80% of the bonus.

### 43. Tip Floor Clamping Overspends on Small Bonus Candidates
- **File**: [`crates/strategy/src/dynamic_tip.rs:167-168`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/strategy/src/dynamic_tip.rs#L167-L168)
- **Description**: `raw.clamp(min_tip_usd, ceiling.max(min_tip_usd))` forces the tip to `min_tip_usd` even when `ceiling < min_tip_usd`.
- **Impact**: On small liquidation bonuses, the bot pays more in tips than the configured `max_tip_pct_of_bonus`, resulting in net negative EV trades.

### 44. Simulator Completely Mocked and Ignores Hurdle Rate
- **File**: [`crates/sim/src/simulator.rs:42-73`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/sim/src/simulator.rs#L42-L73)
- **Description**: `LiteSvmSimulator::simulate()` does not execute instructions against an SVM environment. It checks `routed.expected.net_usd > 0.0` rather than validating against `risk.min_profit_usd` ($8.50).
- **Impact**: Reverting transactions and sub-hurdle trades are incorrectly marked feasible and profitable.

---

## 7. Submission, Latency & Jito Auction Flaws (Wasted Tips & False Confirms)

### 45. Staked QUIC Path Sends HTTP JSON-RPC to Validator TPU Sockets
- **File**: [`crates/submit/src/dual_path.rs:76-93`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/submit/src/dual_path.rs#L76-L93)
- **Description**: Uses `reqwest::Client` to send `sendTransaction` JSON-RPC over HTTP to `tpu_endpoint`. Leader TPU sockets expect raw UDP/QUIC packets, not HTTP JSON-RPC.
- **Impact**: Staked TPU send requests fail with network protocol errors.

### 46. HTTP 200 From Submission Endpoint Treated as Landed On-Chain
- **File**: [`crates/submit/src/dual_path.rs:94-98, 145-149`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/submit/src/dual_path.rs#L94-L98)
- **Description**: Both `StakedQuicSendPath` and `JitoSendPath` treat an HTTP 200 response as `PathOutcome::Landed { actual_net_usd: expected.net_usd }`. HTTP 200 only acknowledges receipt at the RPC/Block Engine.
- **Impact**: Dropped, outbid, or reverted transactions are recorded as profitable, corrupting PnL telemetry and suppressing retries.

### 47. Permanent Route Disablement on Two Reverts Without Auto-Recovery
- **File**: [`crates/strategy/src/breakers.rs:70-74, 84-87`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/strategy/src/breakers.rs#L70-L74)
- **Description**: When a route encounters 2 reverts, `routes_taken_out_of_rotation` permanently disables it. No automated re-arm or exponential backoff exists.
- **Impact**: After minor network turbulence, all profitable routes become permanently disabled until process restart.

### 48. Unchecked Circuit Breaker State in Main Execution Loop
- **File**: [`crates/gyrfalcon-bin/src/main.rs:240-264`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L240-L264)
- **Description**: `breaker_state` is maintained, but `breaker_state.allows()` is never called before sizing or submitting bundles.
- **Impact**: The engine continues firing transactions even when circuit breakers are tripped.

---

## 8. Treasury, Ledger & Dashboard Infrastructure (Capital Leaks & State Drift)

### 49. In-Memory PnL Ledger Discards Drawdown History on Restart
- **File**: [`crates/treasury/src/pnl_ledger.rs:23-38`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/treasury/src/pnl_ledger.rs#L23-L38)
- **Description**: `PnlLedger` stores records in an in-memory `VecDeque` without disk persistence.
- **Impact**: On restart, accumulated losses are wiped out, bypassing the `$750` max drawdown circuit breaker and risking total treasury depletion.

### 50. Dashboard HTML Field Name Mismatch and Hardcoded Thresholds
- **File**: [`dashboard.html:318-330`](file:///c:/Users/pc/Desktop/gyrfalcon/dashboard.html#L318-L330), [`crates/gyrfalcon-bin/src/main.rs:93-101`](file:///c:/Users/pc/Desktop/gyrfalcon/crates/gyrfalcon-bin/src/main.rs#L93-L101)
- **Description**: `dashboard.html` expects `item.profit_usd` and `item.landed`, whereas Rust `LiquidationSummary` provides `net_profit_usd` and `outcome`. Liquidations display as `$0.00 REVERTED`. Additionally, `dashboard.html` hardcodes circuit breaker limits (`profit < -1000`, `reverts >= 3`), ignoring `config/gyrfalcon.toml`.
- **Impact**: Operators see distorted telemetry, incorrect breaker statuses, and inaccurate PnL figures.

---

## Summary Matrix

| Category | Bug Count | Primary Impact |
|---|---|---|
| Core Pipeline & Execution Blockers | 10 | Complete failure to start, connect, or sign transactions |
| Instruction Building & Discriminators | 10 | 100% on-chain reverts across Kamino, Save, MarginFi, and Phoenix |
| DEX Routing & Slippage | 8 | Missing swap instructions, unhandled Token-2022 fees, orphaned router |
| Flash Loan Routing & Sizing | 6 | 1,000,000x under-sizing, wrong offset parsers, fake verification txs |
| Health Factor & Breach Detection | 6 | Late breach detection, stale caches, 100% loss on zero-collateral |
| Strategy & Tip Arbitration | 4 | Stepping loop to 0, non-competitive $0.25 bids, mocked simulation |
| Submission & Latency | 4 | HTTP to UDP TPU, false landed confirms, permanent route lockout |
| Treasury, Ledger & Dashboard | 2 | Loss of drawdown memory on restart, UI display bugs |
| **Total** | **50** | **Unprofitable & Non-functional Trading Engine** |
