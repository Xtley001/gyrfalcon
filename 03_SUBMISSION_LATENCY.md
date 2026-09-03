# 03_SUBMISSION_LATENCY.md
### Owns: how a built bundle gets to a leader, private-orderflow submission,
### tip accounts, and every latency lever between "breach detected" and
### "transaction landed." Does NOT own: how the bundle's instructions are
### built (→ 02_ROUTING_DEX.md, → 01_PROTOCOLS.md), tip sizing/bidding logic
### (→ 04_STRATEGY_RISK.md — this file owns *how* a tip is delivered, not
### *how much*).

---

## 1. "Private mempool" on Solana — what it actually is

Restated from `00_SYSTEM_CONTEXT.md`: Solana has no public mempool, so there
is nothing to be "private" relative to. What you actually want — bundles that
aren't visible to competing searchers before they land, executed atomically,
with a paid-tip auction for leader priority — is the **Jito Block Engine**,
which `crates/submit/src/dual_path.rs` already integrates as one of two
submission paths. This MD hardens and correctly configures that path; it
does not add a second, different system.

## 2. Jito Block Engine — verified endpoints and accounts

**Bundle submission (mainnet):** `https://mainnet.block-engine.jito.wtf`
(already in `config/gyrfalcon.example.toml` as one regional example —
Frankfurt). Jito runs multiple regional block engines; submit to the region
geographically closest to your infrastructure (§4) to minimize the RTT leg
of total latency.

**Tip accounts (mainnet) — verified against Jito's own official docs
(`docs.jito.wtf/lowlatencytxnsend`) and Solana's own developer cookbook
(`solana.com/developers/cookbook/transactions/mev-protection`), cross-checked
against each other and matching exactly:**

```
96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5
HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe
Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY
ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49
DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh
ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt
DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL
3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT
```

**Do not hardcode these as a `const` and forget them.** Jito states these
have remained constant historically, but the contract is: call
`getTipAccounts` on the block engine at startup and on a periodic refresh
interval (define in code, e.g. hourly), and treat the list above as the
fallback/default only if the live call fails. Select one at random per
bundle to spread load, per Jito's own guidance.

**Hard rules from Jito's own docs — violating these silently wastes tip
money:**
- Do **not** put a tip account inside an Address Lookup Table. Jito's docs
  state this explicitly — tips via ALT don't prioritize correctly.
- A tip only matters if the current leader is running Jito-Solana. Tipping
  when the upcoming leader isn't a Jito validator is money burned with zero
  effect. The submission logic must check leader schedule and route
  accordingly — see §3.
- Minimum tip is 1,000 lamports; realistic contested-liquidation tips are
  much higher and must come from `04_STRATEGY_RISK.md`'s empirically-derived
  tip arbitration, not a static number in this file.

**Tip Payment Program (mainnet):** `T1pyyaTNZsKv2WcRAB8oVnk93mLJw2XzjtVYqCsaHqt`
— only relevant if you need to verify a tip transaction's program-level
behavior directly; day-to-day submission goes through the tip accounts
above, not this program ID directly.

## 3. Dual-path submission logic

`crates/submit/src/dual_path.rs` already races Staked QUIC send against Jito
bundle submission. Hardening required:

1. **Leader-awareness for the Jito leg**: before submitting to Jito, check
   whether the next `leaders_ahead` leaders (config value already exists —
   `config/gyrfalcon.example.toml` `[submit] leaders_ahead = 2`) include a
   Jito-Solana validator. If none do within that window, the Jito leg is a
   guaranteed-wasted tip; skip it and rely on Staked QUIC alone for that
   slot window, and log why.
2. **True race, not sequential**: both legs fire concurrently, not
   fire-Jito-then-fallback-to-QUIC. First landed signature wins; the other
   path's in-flight bundle/transaction is left to fail naturally (Solana
   transactions are idempotent per-signature within a blockhash's validity
   window — landing twice isn't a double-liquidation risk if both paths
   carry an identical signed transaction, but confirm this against how
   `dual_path.rs` currently constructs the two paths' payloads before
   assuming it).
3. **Timeout is already configured** (`timeout_ms = 1000`) — verify this
   value against the empirical slot-to-slot outcome data from the replay
   engine rather than leaving it as the placeholder value.

## 4. Colocation and infrastructure latency

- Deploy inference/detection infrastructure in the same region as your
  chosen Jito Block Engine (Frankfurt, Amsterdam, NY, or Tokyo — pick
  nearest to your actual server location, verify current regional endpoint
  list against `docs.jito.wtf` at deploy time since Jito has added regions
  over time).
- Staked QUIC send benefits from network proximity to a high-stake
  validator; use a dedicated low-latency RPC/Geyser provider with a
  presence in the same region rather than a shared public endpoint —
  `config/gyrfalcon.example.toml`'s `[geyser]` and `[staked_send]` sections
  already externalize these as configurable endpoints, so this is a
  provider/region choice, not a code change.
- Everything upstream of submission is already latency-conscious in this
  codebase (Yellowstone gRPC streaming instead of polling, zero-copy
  decode, lock-free ring buffer, in-process LiteSVM sim instead of an RPC
  simulate call) — the remaining latency budget to attack is network RTT
  to the block engine and to the leader, which is an infrastructure
  placement decision, not more code.

## 5. Testable output

- `getTipAccounts` refresh: unit test confirms fallback to the hardcoded
  list in §2 only when the live call fails, and confirms the live list
  replaces it when the call succeeds.
- Leader-awareness: given a synthetic leader schedule with no Jito
  validators in the next `leaders_ahead` slots, confirm the Jito leg is
  skipped and the skip is logged.
- Race semantics: given both paths configured to "succeed" in a sim
  harness, confirm only one landed-outcome is recorded, not two duplicate
  liquidation records in `LiquidationLog`.
