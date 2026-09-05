# gyrfalcon

A deterministic zero-capital liquidation engine for Kamino Lend on Solana.

[![CI](https://img.shields.io/github/actions/workflow/status/Xtley001/gyrfalcon/ci.yml?branch=main)](https://github.com/Xtley001/gyrfalcon/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Rust: 2021/2024](https://img.shields.io/badge/rust-1.81%2B-orange.svg)](https://www.rust-lang.org)
[![Security Policy](https://img.shields.io/badge/security-policy-green.svg)](./SECURITY.md)
[![Docs](https://img.shields.io/badge/docs-spec-blue.svg)](./docs/ARCHITECTURE.md)

`gyrfalcon` monitors Kamino Lend obligations via sub-millisecond Yellowstone Geyser gRPC streaming and executes zero-capital liquidations. It combines zero-copy account decoding, zero-fee flash borrowing (Kamino native with Solend fallback), in-process LiteSVM transaction verification, and dual-path execution across Staked QUIC and the Jito Block Engine. For the theoretical framework, mathematical proofs, and mechanism derivations, see the [whitepaper](./docs/whitepaper.md).

## Installation

```bash
git clone https://github.com/Xtley001/gyrfalcon.git
cd gyrfalcon
cargo build --release
```

## Quickstart

```bash
# Copy example configuration and verify environment readiness
cp config/gyrfalcon.example.toml config/gyrfalcon.toml
cargo run --release --bin readiness-check -- --config config/gyrfalcon.toml

# Start the daemon in observation mode (zero capital at risk)
cargo run --release -- --config config/gyrfalcon.toml --mode observe
```

## Usage

```bash
# Run historical liquidation replay against test fixtures
cargo run --release --bin replay -- --events tests/fixtures/liquidations.jsonl

# Start daemon in production execution mode with local dashboard
cargo run --release -- --config config/gyrfalcon.toml --mode live
```

## Architecture

```
gyrfalcon/
├── crates/
│   ├── core/         # Domain primitives, Protocol enum, trait contracts, Pubkey
│   ├── config/       # Strict TOML schema and runtime configuration validation
│   ├── health/       # Kamino obligation decoders and health factor breach engine
│   ├── ingestion/    # Yellowstone Geyser gRPC streaming client and dispatch ring
│   ├── router/       # Multi-source flash loan routing and 4-venue DEX router
│   ├── strategy/     # Position sizing, dynamic tip curves, and circuit breakers
│   ├── sim/          # LiteSVM in-process simulation and historical replay engine
│   ├── bundler/      # Instruction packing, Token-2022 handling, and ALT manager
│   ├── submit/       # Parallel Staked QUIC leader sockets and Jito bundle transport
│   ├── treasury/     # PnL accounting ledger, wallet floor guards, profit sweep
│   ├── store/        # In-memory position book and non-blocking JSONL audit writer
│   └── gyrfalcon-bin/# Orchestrator daemon runtime and embedded dashboard server
├── config/           # Example configuration templates (mainnet, devnet)
├── deploy/           # Production systemd service units and container configs
├── docs/             # Technical specifications, whitepaper, and operations runbook
└── tests/            # Integration tests and historical fixture datasets
```

For complete pipeline data flow and state machine specifications, see [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md).

## Protocol & Market Reference

| Target | Identifier | Role | Fee / Threshold |
|---|---|---|---|
| **Kamino Lend Program** | `KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD` | Primary Lending Protocol | Discriminator `0xb1479abce2854a37` |
| **Kamino Main Market** | `7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF` | SOL / USDC Lending Market | 85% Liquidation Threshold |
| **Kamino JitoSOL Pool** | `ByYi7nyNQwt5MtG65EHf2N4zpFg46Pchkd1898k7ygFa` | JitoSOL / SOL Correlated Pool | 95% Liquidation Threshold |
| **Kamino Flash Borrow** | `KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD` | Primary Capital Source | 0.00% Fee (12,000 CU) |
| **Solend Flash Borrow** | `So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo` | Fallback Capital Source | 0.00% Fee (15,000 CU) |

## Supported DEX Venues

| Venue | Program ID | Model | Supported Pairs |
|---|---|---|---|
| **Orca Whirlpool** | `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc` | Concentrated Liquidity AMM | `SOL / USDC` |
| **Raydium CLMM** | `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK` | Concentrated Liquidity AMM | `SOL / USDC` |
| **Sanctum** | `5ocnV1qiCgaQR8Jb8xWnVbApNzpWCDveWUig21uT3J9z` | LST Stake Router / Infinity Pool | `JitoSOL / SOL` |
| **Marinade** | `MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD` | Liquid Staking Pool | `mSOL / SOL` |

## Testing

```bash
# Run unit and integration test suite across all 12 workspace crates
cargo test --workspace

# Run historical replay validation against recorded market events
cargo run --bin replay -- --events tests/fixtures/liquidations.jsonl

# Validate environment connectivity and configuration
cargo run --bin readiness-check -- --config config/gyrfalcon.example.toml
```

## Security

Report vulnerabilities per our [Security Policy](./SECURITY.md). Test all deployments in observe mode prior to committing live capital.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for development environment setup, code style standards, and pull request workflows.

## License

Released under the [MIT License](./LICENSE).
