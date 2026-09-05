# Configuration Reference

Complete TOML configuration reference and environment specifications for `gyrfalcon`.

## File Structure

The runtime loads configuration from the path specified by `--config <PATH>` (defaults to `config/gyrfalcon.toml`).

```toml
[protocols.kamino]
enabled = true

[geyser]
url = "https://solana-yellowstone-grpc.triton.one:443"
x_token = "YOUR_GRPC_AUTH_TOKEN"
timeout_ms = 5000

[rpc]
http_url = "https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"
ws_url = "wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY"

[staked_send]
enabled = true
tpu_quic_endpoint = "leader-tpu.solana.com:8003"
connection_pool_size = 4

[jito]
enabled = true
block_engine_url = "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles"
auth_keypair_path = "/etc/gyrfalcon/jito_auth.json"
tip_account = "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5"

[identity]
keypair_path = "/etc/gyrfalcon/signer.json"

[treasury]
pnl_log_path = "/var/log/gyrfalcon/pnl.jsonl"
auto_sweep_enabled = false
sweep_threshold_sol = 10.0
sweep_destination = "YOUR_COLD_WALLET_PUBKEY"

[submit]
mode = "dual_path"
leaders_ahead = 2
dual_path = true
timeout_ms = 2500

[risk]
min_profit_usd = 8.50
max_tip_per_tx_usd = 250.00
max_tip_per_slot_usd = 500.00
max_tip_pct_of_bonus = 0.70
max_drawdown_usd = 5000.00
consecutive_reverts_limit = 3
sync_lag_halt_slots = 5
treasury_floor_sol = 1.0
max_price_impact_bps = 150
```

## Section Parameters

### `[protocols.kamino]`

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `true` | Enables Kamino Lend obligation decoding and liquidation targeting. |

### `[geyser]`

| Field | Type | Default | Description |
|---|---|---|---|
| `url` | `string` | Triton One gRPC endpoint | Yellowstone Geyser gRPC server endpoint URL. |
| `x_token` | `string` | `""` | Bearer authorization token for gRPC stream access. |
| `timeout_ms` | `u64` | `5000` | Stream keepalive ping timeout before initiating reconnection. |

### `[jito]`

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `true` | Enables bundle submission via the Jito Block Engine. |
| `block_engine_url` | `string` | Frankfurt Block Engine | Jito JSON-RPC bundle submission endpoint. |
| `auth_keypair_path` | `string` | `""` | Path to Jito searcher identity keypair file. |
| `tip_account` | `string` | Jito tip account | Solana address of the designated Jito validator tip account. |

### `[risk]`

| Field | Type | Default | Description |
|---|---|---|---|
| `min_profit_usd` | `f64` | `8.50` | Pre-simulation minimum expected profit threshold in USD. |
| `max_tip_pct_of_bonus` | `f64` | `0.70` | Maximum fraction of gross liquidation bonus payable as tip. |
| `consecutive_reverts_limit` | `u32` | `3` | Maximum consecutive reverts before route is locked out. |
| `sync_lag_halt_slots` | `u64` | `5` | Slot lag threshold beyond which all trading halts. |
| `treasury_floor_sol` | `f64` | `1.0` | Minimum SOL balance required in executor keypair. |
| `max_price_impact_bps` | `u16` | `150` | Maximum allowable DEX swap price impact (150 bps = 1.50%). |
