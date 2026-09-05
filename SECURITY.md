# Security Policy

Security invariants, key custody requirements, and vulnerability reporting procedures for `gyrfalcon`.

## Supported Versions

| Version | Supported | Notes |
|---|---|---|
| `0.3.x` | Yes | Active production release (Kamino-only, 4 DEX venues) |
| `0.2.x` | Critical security fixes only | Pre-consolidation multi-venue engine |
| `< 0.2.0` | No | Deprecated |

## Reporting a Vulnerability

Report suspected vulnerabilities privately via [GitHub Security Advisories](https://github.com/Xtley001/gyrfalcon/security/advisories/new). Do not open public issues for security vulnerabilities.

Submissions should include:
- Affected component and commit hash.
- Reproduction script, test case, or transaction payload.
- Quantitative impact assessment (e.g. potential fund loss, execution revert loop, or denial of service).

Expect an initial response and triage within 48 hours.

## Operational Security Invariants

`gyrfalcon` manages transaction signing keypairs and authorizes flash-loan capital. The following rules are hard operational invariants:

- **Key Isolation**: Private keys must never be committed to source control or logged. In production, configure `identity.keypair_path` to an OS-encrypted credential store, hardware security module (HSM), or cloud KMS daemon.
- **Account Sync Staleness**: If the Yellowstone Geyser stream falls behind the network tip by more than `risk.sync_lag_halt_slots` (default 5 slots), the engine halts all execution immediately to prevent executing on stale state.
- **Simulation Gate**: Every liquidation bundle must pass in-process LiteSVM deterministic execution against the slot-current bank before signing. Unsimulated or failing transactions are discarded.
- **Protocol IDL Validation**: Account decoders must match the deployed Anchor IDLs of Kamino Lend (`KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD`). Program upgrades trigger an immediate deployment verification check.
- **Treasury Floor Protection**: If the executor wallet SOL balance drops below `risk.treasury_floor_sol`, trading halts and an operator alert is dispatched.

## Audit Status

This codebase is open-source software provided under the MIT License. Conduct independent formal verification and comprehensive observe-mode testing prior to deploying live capital.
