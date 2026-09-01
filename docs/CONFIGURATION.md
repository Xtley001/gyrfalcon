# Configuration

Every `gyrfalcon` setting lives in `config/gyrfalcon.toml`. Copy the example and edit in place.

```bash
cp config/gyrfalcon.example.toml config/gyrfalcon.toml
```

## Table of Contents

- [Endpoints](#endpoints)
- [Identity](#identity)
- [Protocol toggles](#protocol-toggles)
- [Submission](#submission)
- [Treasury](#treasury)
- [Risk and thresholds](#risk-and-thresholds)
- [Example file](#example-file)

## Endpoints

| Field | Type | Description |
|---|---|---|
| `geyser.url` | string | Leased Yellowstone Geyser gRPC endpoint |
| `geyser.token` | string | Auth token for the Geyser subscription |
| `rpc.url` | string | Fallback RPC for account backfill only, never the hot path |
| `staked_send.url` | string | Leased staked-priority `sendTransaction` endpoint |
| `jito.block_engine_url` | string | Jito block-engine region URL |

## Identity

| Field | Type | Description |
|---|---|---|
| `identity.keypair_path` | string | Path to the signer keypair — never commit this file |
| `identity.tip_account` | string | Jito tip account for bundles |

> Keep `keypair_path` outside the repo. Do not inline private keys into any config committed to version control.

## Protocol toggles

Each protocol adapter is independently enabled. Disable one to halt only its coverage without touching the others.

| Field | Type | Default | Description |
|---|---|---|---|
| `protocols.kamino.enabled` | bool | `true` | Enable the Kamino adapter |
| `protocols.save.enabled` | bool | `true` | Enable the Save adapter |
| `protocols.marginfi.enabled` | bool | `true` | Enable the MarginFi adapter |

## Submission

| Field | Type | Default | Description |
|---|---|---|---|
| `submit.mode` | enum | `observe` | `observe` logs would-be liquidations; `live` submits |
| `submit.leaders_ahead` | int | `2` | Number of upcoming leaders to QUIC-send to |
| `submit.dual_path` | bool | `true` | Run staked QUIC and Jito bundle in parallel |
| `submit.timeout_ms` | int | `1000` | Max milliseconds to race dual submission before timeout |

## Treasury

Funds gas, priority fees, and tips only — never liquidation principal, which is flash-borrowed. See [`STRATEGY.md`](./STRATEGY.md#treasury-and-capital-management).

| Field | Type | Default | Description |
|---|---|---|---|
| `treasury.wallet_path` | string | — | Hot wallet keypair path — never commit this file |
| `treasury.min_balance_sol` | float | operator-set | Below this, all new submissions stop and alert |
| `treasury.sweep_interval` | string | `"1h"` | How often realized profit is swept from settlement back to treasury |

## Risk and thresholds

| Field | Type | Default | Description |
|---|---|---|---|
| `risk.min_profit_usd` | float | `0.0` | Minimum net profit to submit; raise in high-contention regimes |
| `risk.min_tip_usd` | float | `0.0` | Minimum static tip floor (Eq. 2b, $\tau_{\min}$) |
| `risk.sync_lag_halt_slots` | int | operator-set | Sync-lag ceiling before the affected adapter halts (Invariant I2) |
| `risk.contention_ceiling` | float | empirical | Per-reserve write-lock contention cutoff used by the router |
| `risk.max_tip_per_tx_usd` | float | operator-set | Hard tip ceiling per transaction (Eq. 2b, $\tau_{\max}$) |
| `risk.max_tip_per_slot_usd` | float | operator-set | Aggregate tip budget across all candidates fired in one slot |
| `risk.max_tip_pct_of_bonus` | float | operator-set | Tip ceiling as a share of that trade's own bonus (Eq. 2b, $\mu$) |
| `risk.max_drawdown_usd` | float | operator-set | Realized 24h PnL floor before the drawdown breaker halts the engine |
| `risk.consecutive_revert_limit` | int | operator-set | Reverts on the same route before it is pulled from rotation |

## Example file

```toml
[geyser]
url   = "https://your-geyser-endpoint:443"
token = "REPLACE_ME"

[rpc]
url = "https://your-fallback-rpc"

[staked_send]
url = "https://your-staked-send-endpoint"

[jito]
block_engine_url = "https://frankfurt.mainnet.block-engine.jito.wtf"

[identity]
keypair_path = "/secrets/gyrfalcon-signer.json"
tip_account  = "REPLACE_ME"

[treasury]
wallet_path      = "/secrets/gyrfalcon-treasury.json"
min_balance_sol  = 2.0
sweep_interval   = "1h"

[protocols.kamino]
enabled = true

[protocols.save]
enabled = true

[protocols.marginfi]
enabled = true

[submit]
mode          = "observe"
leaders_ahead = 2
dual_path     = true
timeout_ms    = 1000

[risk]
min_profit_usd          = 5.0
min_tip_usd             = 0.0
sync_lag_halt_slots     = 3
contention_ceiling      = 0.8
max_tip_per_tx_usd      = 150.0
max_tip_per_slot_usd    = 600.0
max_tip_pct_of_bonus    = 0.4
max_drawdown_usd        = 1000.0
consecutive_revert_limit = 3
```
