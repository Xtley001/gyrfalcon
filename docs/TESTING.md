# Testing

`gyrfalcon` is validated in three layers: unit tests per crate, historical replay of real liquidation events, and per-route compute profiling. Correctness of the adapters and the sync pipeline is proven before latency is ever measured.

## Table of Contents

- [Unit tests](#unit-tests)
- [Historical replay](#historical-replay)
- [Per-route CU profiling](#per-route-cu-profiling)
- [Devnet dry-run](#devnet-dry-run)

## Unit tests

```bash
cargo test --workspace
```

Decoders are tested against fixture accounts captured from mainnet so a silent layout change surfaces as a failing test rather than a wrong health-factor number.

## Historical replay

Replays real historical liquidation events across all three protocols and asserts the engine would have caught them correctly. This is the [first action](./RUNBOOK.md#first-action) — nothing downstream is worth building until it passes.

```bash
cargo run --release --bin replay -- --events tests/fixtures/liquidations.jsonl
```

Each replayed event asserts three things:

- **Detection** — the health engine flagged the breach on time.
- **Routing** — the router picked a reserve with actually-sufficient depth at that historical moment, not headline TVL.
- **Feasibility** — the CU and ALT budget for that specific route actually fit under the 1232-byte and CU ceilings.

A failure on any axis for known-good historical data blocks promotion.

## Per-route CU profiling

Every `(protocol × collateral × debt × swap-venue)` route in scope is run through LiteSVM to record actual compute consumption — real numbers, not estimates. The table it produces feeds both `setComputeUnitLimit` and priority-fee bidding math.

```bash
cargo run --release --bin cu-profile -- --out tests/fixtures/cu_table.csv
```

| Column | Meaning |
|---|---|
| `route_id` | Protocol × collateral × debt × swap venue |
| `cu_measured` | Actual CU consumed in LiteSVM |
| `cu_limit_set` | Tight limit written to the transaction |
| `tx_bytes` | Serialized v0 transaction size under its warmed ALT |
| `fits` | Whether the route clears all hard constraints |

Rebuild this table whenever routes change or a protocol upgrades.

## Devnet dry-run

Before real liquidation capital is committed, run the full staked-send + Jito dual submission end-to-end on devnet or at small mainnet size.

```bash
cargo run --release -- --config config/gyrfalcon.devnet.toml --mode live
```

This exercises the submission path — leader-schedule lookup, staked QUIC send, and Jito bundle in parallel — against a live network without meaningful capital at risk.
