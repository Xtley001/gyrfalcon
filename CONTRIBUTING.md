# Contributing

Development guidelines and standards for `gyrfalcon`.

## Development Setup

Prerequisites: Rust 1.81+, Solana CLI 2.x, and a local or remote Yellowstone Geyser gRPC endpoint.

```bash
git clone https://github.com/Xtley001/gyrfalcon.git
cd gyrfalcon
cargo build --workspace
cargo test --workspace
```

Verify formatting and linting prior to committing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Architectural Standards

- **Zero-Copy Deserialization**: Decode on-chain account data with zero allocations using `bytemuck` or fixed binary offsets. Never allocate intermediate objects on the hot path.
- **Deterministic Simulation**: All execution paths must clear in-process LiteSVM verification before signing. No transaction may be submitted optimistically without prior simulation.
- **Fault Isolation**: Health decoders and submission threads must isolate panics. Failures in one market or route must trigger circuit breakers rather than destabilizing the daemon process.
- **Asynchronous Hot Path**: The pipeline from Yellowstone ingestion to submission must avoid disk I/O and blocking locks. Logging is offloaded to `gyrfalcon-store` via bounded mpsc channels.

## Pull Request Process

1. Create a descriptive feature branch (`git checkout -b feature/sanctum-slippage-optimization`).
2. Implement changes with corresponding unit tests or fixture updates in `tests/fixtures/`.
3. Verify the full workspace passes tests, replay simulation, and lints:
   ```bash
   cargo test --workspace
   cargo run --bin replay -- --events tests/fixtures/liquidations.jsonl
   ```
4. Update [CHANGELOG.md](./CHANGELOG.md) under the `[Unreleased]` section.
5. Open a pull request with a concise description of performance or correctness impacts.

## Commit Style

Use concise, present-tense imperative subject lines under 72 characters:
- `add Sanctum stake route dynamic quote validation`
- `fix Kamino obligation liquidity calculation for Token-2022`
- `update LiteSVM simulation hurdle rate to $8.50 floor`
