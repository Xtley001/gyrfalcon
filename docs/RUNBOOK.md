# Operations Runbook

Operational procedures, deployment requirements, and incident response runbook for `gyrfalcon`.

## Infrastructure Requirements

### Recommended Hardware

For low-latency execution and determinism, deploy on bare-metal or high-priority cloud instances:

| Component | Minimum Specification | Recommended Specification |
|---|---|---|
| **CPU** | 8 cores, 3.5 GHz+ (x86_64) | AMD EPYC 9354 or Ryzen 9 7950X (16+ cores) |
| **RAM** | 32 GB DDR5 ECC | 64 GB DDR5 ECC |
| **Storage** | 256 GB NVMe SSD | 1 TB Enterprise NVMe (PCIe 4.0+) |
| **Network** | 1 Gbps symmetric | 10 Gbps uplink, low jitter (< 1 ms to IX) |
| **Deployment Region** | Europe Central | **AWS `eu-central-1` (Frankfurt)** or **Hetzner Falkenstein** |

*Note: Deployment in Frankfurt minimizes RTT to Jito Block Engine relays and European validator clusters.*

## Deployment Phases

### Phase 0: Observation Mode (Zero Capital Risk)

Always run the engine in `observe` mode for at least 48 hours to confirm zero-copy decoder fidelity, simulate liquidation volume, and calibrate slippage without signing transactions:

```bash
# 1. Run environment readiness audit
cargo run --release --bin readiness-check -- --config config/gyrfalcon.toml

# 2. Start daemon in observation mode
./target/release/gyrfalcon --config config/gyrfalcon.toml --mode observe
```

Verify in observation logs:
- Yellowstone gRPC stream maintains sub-slot delivery (`sync_lag == 0`).
- LiteSVM simulations emit `SimResult.profitable == true` on real market breaches.
- WebSocket metrics dashboard (`http://localhost:8080`) reports healthy telemetry.

### Phase 1: Live Execution

Once observation validation passes:

```bash
# Verify signer wallet balance exceeds treasury floor
solana balance <SIGNER_PUBKEY> --url https://api.mainnet-beta.solana.com

# Start daemon in live execution mode
./target/release/gyrfalcon --config config/gyrfalcon.toml --mode live
```

## Systemd Service Configuration

Deploy using a dedicated systemd service (`/etc/systemd/system/gyrfalcon.service`):

```ini
[Unit]
Description=Gyrfalcon Kamino Liquidation Engine
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=gyrfalcon
WorkingDirectory=/opt/gyrfalcon
ExecStart=/opt/gyrfalcon/target/release/gyrfalcon --config /opt/gyrfalcon/config/gyrfalcon.toml --mode live
Restart=always
RestartSec=3
LimitNOFILE=65536
CPUQuota=400%
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable gyrfalcon
sudo systemctl start gyrfalcon
sudo journalctl -u gyrfalcon -f
```

## Incident Response & Troubleshooting

| Alert / Symptom | Root Cause | Immediate Action |
|---|---|---|
| `SyncLagHalt` triggered | Yellowstone gRPC stream lagging $> 5$ slots | Check Triton/Helius endpoint status; failover to secondary provider. |
| `ConsecutiveReverts` ($\ge 3$) | Unmodeled on-chain slippage or priority fee escalation | Inspect recent signatures on Solscan; route enters automatic 300-slot cooldown. |
| `TreasuryFloorBreached` | Wallet SOL balance $< 1.0$ SOL | Transfer SOL to executor keypair; engine resumes automatically upon next balance poll. |
| `LiteSvmHurdleReject` | Spread after fees $< \$8.50$ | Normal behavior; low-spread candidates are safely discarded before submission. |

## Emergency Stop Procedure

To halt all trading and cancel pending executions immediately:

```bash
# Kill active process
sudo systemctl stop gyrfalcon

# Verify process has terminated
pgrep -fl gyrfalcon
```
