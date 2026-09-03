# Changelog

All notable changes to `gyrfalcon` are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-09-03

### Added
- **End-to-End Position Sizing & Routing**: Added `size_and_route` connecting obligation breach detection to profitable route selection net of fees, slippage, and tips.
- **Dedicated Flash Loan Builders**: Added dedicated Kamino flash borrow/repay and MarginFi borrow/repay instruction builders with exact Anchor discriminators.
- **Extended DEX Venue Coverage**: Added Raydium CPMM (`raydium_cp`) builder, Jupiter v6 fallback swap builder, and Phoenix CLOB seat account support.
- **Multi-Program ATA Handling**: Added `required_atas_with_programs` and `create_all_atas_with_programs_idempotent` supporting heterogeneous token programs (SPL Token + Token-2022).
- **Disk Persistence for Realized PnL**: Added JSON `snapshot` and `restore` methods to `PnlLedger` in `gyrfalcon-treasury`.
- **Route Lockout Recovery**: Added `rearm_all_routes` to `BreakerState` in `gyrfalcon-strategy`.

### Fixed
- **Instruction Discriminators**: Corrected Anchor discriminators for Kamino liquidation (`0xb1479abce2854a37`) and MarginFi liquidation (`0xd6a997d5fba756db`).
- **Account Layouts**: Fixed Save liquidation instruction to provide all 15 required accounts; derived Kamino `lending_market_authority` PDA and oracle accounts.
- **DEX Swaps**: Added `amm_config` and `tick_arrays` to Raydium CLMM; added `bin_arrays` to Meteora DLMM.
- **Binary Offsets**: Fixed MarginFi `Bank` memory offsets (`group` at 8..40, `mint` at 40..72, liquidity at 72..80).
- **Decimal Scaling**: Scaled Save `borrowed_amount_wads` to base native units ($10^{\text{mint\_decimals}}$) in `close_factor_max_repay`.
- **Account Disambiguation**: Disambiguated Save accounts by exact length (`Reserve: 619`, `Obligation: 1300`, `LendingMarket: 290`) preventing false `LendingMarket` matches.
- **Zero-Collateral Vulnerabilities**: Added zero-collateral protection across Kamino, Save, and MarginFi to prevent liquidating zero-collateral positions.
- **MarginFi Close Factor**: Capped MarginFi liquidation close factor at 50% to prevent program reverts.
- **Competitive Dynamic Tip Bidding**: Scaled static and dynamic tips with bonus size up to caps rather than clamping to floor; fixed small-bonus tip floor clamping.
- **Simulation Hurdle Rate**: Enforced minimum hurdle rate ($8.50 net USD) in `LiteSvmSimulator::simulate`.
- **Submission Outcomes**: Differentiated RPC HTTP 200 acceptance from on-chain execution in `DualPathSubmitter`.
- **Daemon Orchestration**: Replaced dummy 64-zero transaction buffer with live `gyrfalcon_bundler::assemble` v0 compilation and configured endpoints.
- **Workspace Reintegration**: Re-enabled all 12 workspace crates in root `Cargo.toml`.

## [0.1.0] - 2026-09-01

### Added
- **Multi-Protocol Health Adapters**: Full obligation and account decoders for Kamino Lend (`klend-interface 0.6`), Save / Solend (`solend-sdk 0.1`), and MarginFi v2 (`marginfi-type-crate`).
- **LiteSVM Simulation Harness**: In-process deterministic execution simulation implementing `gyrfalcon_core::traits::Simulator` with exact compute unit profiling and feasibility validation.
- **Instruction Bundler & Swaps**: Automated liquidation instruction generation and DEX swap instruction builders for Raydium CLMM, Orca Whirlpools, and Meteora DLMM.
- **Token-2022 Support**: Added SPL Token-2022 program support and transfer-fee extension calculations.
- **ALT Manager**: Address Lookup Table manager creating, extending, and caching lookup tables for high-velocity liquidation mint pairs.
- **Live Ingestion**: Yellowstone Geyser gRPC streaming client with automatic reconnect backoff and $O(1)$ `HashSet` decoder registry dispatch.
- **Dual-Path Submission**: Parallel submission transports for Staked QUIC (direct to TPU leader sockets) and Jito Block Engine JSON-RPC bundle submission.
- **Async Persistence**: Non-blocking `AsyncLiquidationWriter` logging liquidation outcomes over bounded channels without hot-path latency penalties.
- **Orchestrator Daemon & Dashboard Server**: Multi-threaded pipeline runtime with embedded Axum WebSocket/HTTP server streaming real-time metrics to `dashboard.html`.

### Fixed
- Fixed critical `close_factor()` bug across Kamino, Save, and MarginFi adapters returning tracking slot numbers instead of close factors.
- Fixed MarginFi liability sizing calculation evaluating bank share conversions (previously hardcoded to zero).
- Fixed `Pubkey::from_base58` accepting short and malformed strings by enforcing strict 32-byte length verification.
- Fixed `BreakerState::record_success` route rotation synchronization.
- Replaced hand-rolled base64 implementations across ingestion and sim crates with standard `base64::prelude::BASE64_STANDARD`.
- Upgraded root workspace dependencies (`toml 0.8`, `clap 4.5`, `solana-sdk 2.x`, `litesvm 0.7.1`).
