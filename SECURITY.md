# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 0.1.x | Yes |
| 1.0.x | Yes |

## Reporting a Vulnerability

Report suspected vulnerabilities privately by opening a [GitHub security advisory](https://github.com/Xtley001/gyrfalcon/security/advisories/new). Do not open a public issue for a security report. Expect an initial acknowledgment within a few days.

Include, where possible: affected component, reproduction steps, and impact assessment.

## Operational Security

`gyrfalcon` handles a signer keypair and submits transactions that move value. The following are treated as security requirements, not best-effort:

- **Signer keys never enter version control.** `keypair_path` points outside the repo; committed config carries placeholders only.
- **The account-sync pipeline is the single point of failure.** If it drifts stale, every decision is computed against a world that no longer exists. Staleness detection is mandatory; sync lag past threshold halts the affected adapter.
- **Decoders are pinned to deployed IDLs.** A stale account layout produces silently wrong numbers, not a visible crash. Re-verify on every protocol upgrade.
- **Simulation gates submission.** No transaction is submitted unless it clears LiteSVM against a slot-current account set and shows positive net profit.

### Key custody

A plain, unencrypted keypair file on the same box that runs the engine is the minimum viable setup for `observe` mode or devnet testing only. Before `live` mode with real capital, use one of: an OS-level encrypted keychain, a hardware security module (HSM), or a cloud KMS that releases the key to the signing process without ever writing it to disk in cleartext. The treasury wallet — smaller balance, more frequently accessed — can reasonably use a lighter-weight option than the signer identity key if the two are separated, but neither should be a bare JSON file on a shared or internet-facing host.

### Dependency vetting

The single point of failure described above sits on top of [LiteSVM](https://github.com/LiteSVM/litesvm), a comparatively young project. Pin an exact, audited-by-you commit or release tag rather than tracking its main branch, and re-run the full historical replay suite (`docs/TESTING.md#historical-replay`) after every version bump before promoting to `live` — a silent behavior change in the simulator is indistinguishable from a silent behavior change in the account-sync pipeline it depends on.

## Audit Status

This code has not been audited. Do not commit real liquidation capital without your own review and completion of the [production readiness checklist](./docs/RUNBOOK.md#production-readiness-checklist).
