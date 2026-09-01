# Contributing

Guidelines for contributing to `gyrfalcon`.

## Development Setup

```bash
git clone https://github.com/Xtley001/gyrfalcon.git
cd gyrfalcon
cargo build --workspace
cargo test --workspace
```

Format and lint before opening a pull request:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

## Adapter & Execution Rules

- **Zero-Copy Deserialization**: Use `bytemuck` and raw struct casting where account layouts allow.
- **Oracle Validation**: Handle oracle staleness and confidence bounds strictly per protocol.
- **Fault Isolation**: Panic or error in one adapter must never impact other protocol pipelines.
- **In-Process Hot Path**: The execution pipeline (`Ingestion → Health → Strategy → Sim → Bundler`) must remain non-blocking and in-process.

## Pull Requests

- Keep PRs focused on single logical changes.
- Ensure all unit tests and simulation checks pass.
- Update `CHANGELOG.md` with every user-facing change.

## Commit style

Use present-tense, imperative subject lines under ~72 characters (`add MarginFi oracle confidence check`). Reference the issue number where one exists.
