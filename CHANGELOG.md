# Changelog

All notable changes to `gyrfalcon` are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-09-05

### Added
- **Single-Protocol Focus**: Concentrated execution engine exclusively on Kamino Lend (`KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD`) across Main Market (`7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF`) and JitoSOL Correlated Pool (`ByYi7nyNQwt5MtG65EHf2N4zpFg46Pchkd1898k7ygFa`).
- **Four-Venue Direct DEX Routing**: Added dedicated direct swap execution for Orca Whirlpools, Raydium CLMM, Sanctum (JitoSOL/SOL unstake/swap), and Marinade (mSOL/SOL unstake/swap).
- **Fallback Flash-Loan Routing**: Implemented fallback flash loan routing to Solend (`So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo`) when Kamino native flash reserves lack required depth.
- **Dynamic Congestion Tip Curve**: Added mathematical tip bidding curve $\rho(c) = \frac{c^k}{1 + c^k}$ responsive to write-lock slot contention and three discrete gas regimes (`Normal`, `High`, `Spike`).
- **DeFi-Grade Documentation Suite**: Added comprehensive formal whitepaper (`docs/whitepaper.md`), operational runbook (`docs/RUNBOOK.md`), API specification (`docs/API.md`), architecture deep-dive (`docs/ARCHITECTURE.md`), and configuration reference (`docs/CONFIGURATION.md`).

### Changed
- **Workspace Architecture**: Collapsed `Protocol` enum to `{ Kamino }` and pruned multi-protocol abstraction layers for zero-overhead hot-path decoding.
- **Config Schema**: Simplified `ProtocolsConfig` in `crates/config` to `{ pub kamino: ProtocolToggle }`.
- **Replay Dataset**: Refactored `tests/fixtures/liquidations.jsonl` and `cu_table.csv` to strictly genuine Kamino liquidation events.
- **Simulation Validation**: Enforced post-simulation $8.50 net USD profit hurdle rate in `LiteSvmSimulator`.

### Removed
- Removed Save/Solend and MarginFi v2 obligation health decoders (`save.rs`, `marginfi.rs`).
- Removed legacy DEX swap builders for Meteora DLMM, Phoenix CLOB, and Raydium CP Swap.
- Removed orphaned `cu-profile.rs` binary.

## [0.2.0] - 2026-09-03

### Added
- **End-to-End Position Sizing & Routing**: Added `size_and_route` connecting obligation breach detection to profitable route selection net of fees, slippage, and tips.
- **Dedicated Flash Loan Builders**: Added dedicated Kamino flash borrow/repay and MarginFi borrow/repay instruction builders with exact Anchor discriminators.
- **Multi-Program ATA Handling**: Added `required_atas_with_programs` and `create_all_atas_with_programs_idempotent` supporting heterogeneous token programs (SPL Token + Token-2022).
- **Disk Persistence for Realized PnL**: Added JSON `snapshot` and `restore` methods to `PnlLedger` in `gyrfalcon-treasury`.
- **Route Lockout Recovery**: Added `rearm_all_routes` to `BreakerState` in `gyrfalcon-strategy`.

### Fixed
- **Instruction Discriminators**: Corrected Anchor discriminators for Kamino liquidation (`0xb1479abce2854a37`) and MarginFi liquidation (`0xd6a997d5fba756db`).
- **Account Layouts**: Fixed Save liquidation instruction to provide all 15 required accounts; derived Kamino `lending_market_authority` PDA and oracle accounts.
- **DEX Swaps**: Added `amm_config` and `tick_arrays` to Raydium CLMM; added `bin_arrays` to Meteora DLMM.
- **Decimal Scaling**: Scaled Save `borrowed_amount_wads` to base native units ($10^{\text{mint\_decimals}}$) in `close_factor_max_repay`.
- **Zero-Collateral Vulnerabilities**: Added zero-collateral protection to prevent attempting liquidation on empty collateral accounts.
- **Competitive Dynamic Tip Bidding**: Scaled static and dynamic tips with bonus size up to caps rather than clamping to floor.
- **Simulation Hurdle Rate**: Enforced minimum hurdle rate ($8.50 net USD) in `LiteSvmSimulator::simulate`.
- **Submission Outcomes**: Differentiated RPC HTTP 200 acceptance from on-chain execution in `DualPathSubmitter`.

## [0.1.0] - 2026-09-01

### Added
- **Multi-Protocol Health Adapters**: Initial obligation and account decoders for Kamino Lend, Save / Solend, and MarginFi v2.
- **LiteSVM Simulation Harness**: In-process deterministic execution simulation implementing `gyrfalcon_core::traits::Simulator`.
- **Instruction Bundler & Swaps**: Automated liquidation instruction generation and DEX swap instruction builders.
- **Token-2022 Support**: Added SPL Token-2022 program support and transfer-fee extension calculations.
- **ALT Manager**: Address Lookup Table manager creating, extending, and caching lookup tables for high-velocity liquidation mint pairs.
- **Live Ingestion**: Yellowstone Geyser gRPC streaming client with automatic reconnect backoff.
- **Dual-Path Submission**: Parallel submission transports for Staked QUIC and Jito Block Engine JSON-RPC bundles.
- **Async Persistence**: Non-blocking `AsyncLiquidationWriter` logging liquidation outcomes over bounded channels.
- **Orchestrator Daemon & Dashboard Server**: Multi-threaded pipeline runtime with embedded Axum WebSocket/HTTP server streaming real-time metrics to `dashboard.html`.
