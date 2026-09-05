# Data & Testing Policy

Standards for test fixtures, simulation replay datasets, and privacy policies in `gyrfalcon`.

## Zero-Mock Testing Invariant

Financial calculations, liquidation sizing, and compute unit limits must never be tested using fabricated or arbitrarily mocked data structures. Inaccurate test data leads to catastrophic false positives during live execution.

The codebase strictly enforces:
- **Genuine Account Layouts**: All serialized account data used in unit tests must be dumped directly from Solana Mainnet-Beta RPC or Yellowstone gRPC streams.
- **Accurate Discriminators**: Instruction discriminators and error codes must match deployed Anchor programs byte-for-byte.
- **Realistic Slippage Curves**: DEX pool depths and reserves must reflect real liquidity distributions on Orca, Raydium, Sanctum, and Marinade.

## Historical Replay Datasets

The repository maintains historical liquidation fixtures under `tests/fixtures/`:

| Dataset | Path | Description |
|---|---|---|
| `liquidations.jsonl` | `tests/fixtures/liquidations.jsonl` | Mainnet Kamino liquidation events recorded with exact slot, repay amount, and seized collateral. |
| `cu_table.csv` | `tests/fixtures/cu_table.csv` | Empirically profiled compute unit consumption for Kamino instructions, flash loans, and DEX swaps. |

### Integrity Requirements

1. **No Synthetic Padding**: Never generate artificial records to achieve arbitrary test count targets.
2. **Kamino-Exclusive Scope**: Fixtures must reference Kamino Lend (`KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD`) and the 4 supported DEX venues only.
3. **Replay Validation**: Every commit must pass the historical replay runner:
   ```bash
   cargo run --bin replay -- --events tests/fixtures/liquidations.jsonl
   ```

## Key & Credential Hygiene

- **Zero Secrets in Repository**: No private keys, keypairs, RPC API tokens, or gRPC bearer tokens may ever be committed to git.
- **Placeholder Enforcement**: Example configuration templates (`config/*.example.toml`) must contain explicit placeholder strings (`YOUR_KEY`, `YOUR_GRPC_AUTH_TOKEN`).
- **Log Scrubbing**: Non-blocking loggers in `gyrfalcon-store` must never serialize private keys or sensitive signer seed material.
