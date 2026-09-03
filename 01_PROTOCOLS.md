# 01_PROTOCOLS.md
### Owns: which lending protocols this system reads and liquidates against, their
### verified program IDs, and each protocol's flash-loan readiness status.
### Does NOT own: swap routing (→ 02_ROUTING_DEX.md), sizing logic (→ 04_STRATEGY_RISK.md).

---

## 1. Verified program IDs (mainnet-beta)

Every ID below is already present and in active use in this codebase
(`README.md` table + `crates/health/src/adapters/`, `crates/router/src/lib.rs`)
or independently confirmed against the protocol's own docs during this spec
pass. Do not add a protocol below this line without the same standard: a
citation to the protocol's own docs/repo, not a third-party aggregator page.

| Protocol | Program ID | Source of truth |
|---|---|---|
| Kamino Lend | `KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD` | Already wired in repo (`klend-interface`) |
| Save (formerly Solend) | `So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo` | Already wired in repo (`solend-sdk`) |
| MarginFi v2 | `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` | Already wired in repo; cross-confirmed against Solana's official verified-builds documentation |
| Drift Protocol v2 | `dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH` | Drift's own official docs (`docs.drift.trade/about-v2/program-vault-addresses`) |

**Not in this system, and not to be added without a new decision logged in
`DECISIONS.md`:** any Solana lending market not listed above. Several older
Solana money markets from prior cycles no longer carry meaningful borrow
volume or breach flow — building and maintaining an adapter for a market with
no liquidations to catch is wasted engineering effort. If you believe one
should be added, the acceptance bar is: current TVL, current borrow volume,
and evidence of recent on-chain liquidation events, cited from the
protocol's own dashboard or a block explorer — not a memory of what was
active in a prior cycle.

## 2. Per-protocol readiness (what "done" means for each)

### Kamino Lend — `KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD`
- Health decode: done (`crates/health/src/adapters/kamino.rs`)
- Flash loan: native instruction introspection, already routed
  (`crates/router/src/lib.rs`)
- Liquidation mechanic: Kamino uses **soft (partial) liquidations** — a
  breached position is repaid partially, not seized all at once, with the
  liquidation discount scaling as the position deteriorates further.
  `size_position()` and the health adapter must both reflect this — do not
  assume a single full-close event per breach; the same obligation can
  legitimately re-breach and be liquidated again.
- Status: **production-ready pending §2 general items in `00_SYSTEM_CONTEXT.md`**
  (route-feasibility stepping, empirical risk params).

### Save — `So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo`
- Health decode: done (`crates/health/src/adapters/save.rs`)
- Flash loan: native reserve flash borrow/repay, routed — **but flagged
  functionally limited by Save's own documentation.**
- **Acceptance criteria before Save is allowed to size a live liquidation
  off a Save-sourced flash loan:** a devnet or small-size mainnet dry run
  where a Save flash borrow + repay lands in one transaction, observed and
  logged, with the transaction signature recorded in `DECISIONS.md`. Until
  that exists, `MultiSourceRouter` may rank Save reserves for depth/fee
  comparison, but the executor must not select Save as the actual flash
  source for a live-mode liquidation — fall through to Kamino.

### MarginFi v2 — `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA`
- Health decode: done (`crates/health/src/adapters/marginfi` — verify path
  against actual repo layout, do not assume)
- Flash loan: native flash loan instructions claimed, **unverified in this
  codebase per the router's own doc comment.**
- Same acceptance bar as Save above: a logged, verified test transaction
  before MarginFi is a live flash source.
- Liquidation fee note: MarginFi liquidations carry a fee split between the
  liquidator and the bank's insurance fund — confirm the current split and
  bank-specific parameters from the account data at breach time, not a
  hardcoded constant, since it can vary by bank.

### Drift Protocol v2 — `dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH` (NEW)
- Not yet in this codebase. This is the one genuinely new protocol worth
  adding — distinct mechanics from the other three:
  - Cross-margin: **one collateral pool backs all of a user's positions**
    (spot + perp), unlike Kamino/Save/MarginFi's per-position or per-bank
    model. A health-factor breach here is account-level, not position-level.
  - Liquidation waterfall is different: liquidations route first to a
    backstop AMM, then to a community insurance fund if the backstop can't
    absorb it. This is not a flash-loan-and-swap pattern like the other
    three — it needs its own adapter and its own execution path, not a
    bolt-on to the existing `HealthAdapter` trait's assumptions unless that
    trait already generalizes cleanly. Check `crates/core/src/traits.rs`
    before assuming it does.
  - New crate work required: `crates/health/src/adapters/drift.rs` (account
    decode + breach calc against Drift's spot market and perp market state),
    plus a Drift-specific liquidation instruction builder in
    `crates/bundler/`. This does not reuse the flash-loan router at all —
    Drift liquidations don't work via external flash loan the way the other
    three do; confirm this against Drift's own program docs before writing
    the instruction builder, don't assume.
- **Acceptance criteria:** observe-mode run showing correctly decoded Drift
  account health matching what Drift's own UI/API reports for the same
  account, before any liquidation instruction is built.

## 3. Health-factor decode completeness checklist

For every protocol above, the health adapter must handle, not just the
happy-path fully-collateralized-to-breached case:
- Isolated vs. cross/global account modes (MarginFi has both; confirm which
  Kamino and Save reserves are isolated-tier vs. main-pool before treating
  all reserves the same way for sizing)
- e-mode / correlated-asset weighting where the protocol has it (affects the
  actual liquidation threshold, not just the display LTV)
- Oracle staleness — Pyth publishes sub-second, but a stale or absent oracle
  update must halt breach evaluation for that account, not fall back to a
  cached price silently

## 4. What was considered and rejected for this system, and why

- Other historical Solana money markets: no current liquidation flow worth
  the maintenance cost — see §1.
- Any EVM lending market: out of scope per `00_SYSTEM_CONTEXT.md §2`.
