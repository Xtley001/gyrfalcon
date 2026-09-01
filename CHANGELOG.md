# Changelog

All notable changes to `gyrfalcon` are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
