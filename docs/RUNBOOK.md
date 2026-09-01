# Runbook

Operational guide for deploying and running `gyrfalcon`. Read the [production readiness checklist](#production-readiness-checklist) before committing any capital.

## Table of Contents

- [Deployment phases](#deployment-phases)
- [Hardware](#hardware)
- [Manual kill switch](#manual-kill-switch)
- [Operating cost model](#operating-cost-model)
- [First action](#first-action)
- [Production readiness checklist](#production-readiness-checklist)
- [Incident response](#incident-response)
- [Where this breaks](#where-this-breaks)

## Deployment phases

No validator is needed to run this system. A full voting validator requires real stake, higher hardware tiers, and slashing exposure that buys nothing at this scale. What is needed is a fast, well-placed Geyser consumer plus a leased staked-send endpoint.

### Phase 0 — prove correctness

Correctness of the three adapters and the LiteSVM sync pipeline, not latency.

- Cloud VM
- Leased Yellowstone gRPC subscription
- Leased staked-send endpoint (same provider where available), to start measuring real landing rates early
- Run in `observe` mode

### Phase 1 — production

Only after historical backtests confirm the adapters predict real liquidations on time.

- Bare-metal, single box, colocated near the leased provider's low-latency PoP (commonly Frankfurt, Amsterdam, Tokyo, or the NY area — confirm against the actual provider, not assumed generically)
- Promote to `live` mode
- Multi-region only once data shows races lost specifically to geography — not to tip sizing or a route that does not fit the CU/size budget

## Hardware

Spec priorities in order:

- **Single-thread clock speed** first — Geyser decode, LiteSVM execution, and CU accounting are latency-sensitive per-item work, not embarrassingly parallel.
- **RAM** second — the account store for all reserves, oracles, and obligations across three protocols is small; low tens of GB is generous.
- **NIC quality and jitter** over raw bandwidth, third.

Kernel-level tuning: CPU pinning and isolation for hot-path threads, disabled frequency scaling, and a tuned network stack for QUIC send.

## Manual kill switch

The four circuit breakers in [`STRATEGY.md`](./STRATEGY.md#circuit-breakers) are automatic and condition-triggered. Independent of all of them, the operator can force-halt all new submissions by hand at any time, regardless of breaker state — `submit.mode` set to `observe` (or a dedicated `--halt` flag) takes effect on the next evaluation cycle without waiting for a breaker condition to be met. This is the tool of first resort for "something looks wrong and I'm not sure what yet" — don't wait for a breaker to confirm the problem before pulling it.

## Operating cost model

Nothing in this system is free to run, and the realistic edge (Whitepaper Appendix A: isolated/awkward-collateral routes, not blue-chip pools) is thin enough that infrastructure cost matters to whether the strategy clears at all.

| Cost | Cadence | Notes |
|---|---|---|
| Leased Yellowstone Geyser subscription | Monthly | Priced by provider and region; staked-priority tiers cost more than best-effort |
| Leased staked-send endpoint | Monthly | Often bundled with the Geyser lease by the same provider |
| Jito tips | Per landed bundle | Variable, scales with contention (Whitepaper Eq. 2b) — not a fixed line item |
| Colocated bare-metal (Phase 1) | Monthly | Only after Phase 0 proves correctness; see [Deployment phases](#deployment-phases) |
| Treasury float | One-time + replenished | Capital sitting in the hot wallet earning nothing while it waits to fund gas/tips |

Before Phase 1 colocation spend, confirm Phase 0's `observe`-mode data suggests realized net profit would clear the fixed monthly cost of the leases plus colocation — not just that individual candidates score positive under Equation 2.

## First action

The single most important checkpoint in the whole build — expanded into the full staged sequence in [`docs/BUILD_ORDER.md`](./BUILD_ORDER.md). Before writing the bundler or touching Jito: build the flash-source router and the LiteSVM sync pipeline first, and validate both against real historical liquidation events across all three protocols:

- Did the health engine flag them on time?
- Did the router pick a reserve with actually-sufficient depth at that historical moment?
- Did the CU/ALT budget for that specific route actually fit?

If any of these three fail on known-good historical data, nothing downstream is worth building yet.

## Production readiness checklist

Confirm each against live program state directly, not documentation, before any capital is at risk.

- [ ] Current IDL for Kamino, Save, and MarginFi pulled from the deployed program, not a cached copy
- [ ] Kamino and Save flash-borrow fee schedules confirmed at current bps, per reserve
- [ ] MarginFi flash-loan support re-confirmed as absent (or present) before the router ships without a MarginFi branch
- [ ] Oracle staleness/confidence handling verified per protocol
- [ ] Per-route CU profiling table built from real LiteSVM runs
- [ ] ALTs built and warmed for every route intended to fire on day one
- [ ] Empirical write-lock contention map built from observed slot data for the reserves actually being watched
- [ ] ATA pre-provisioning complete for every mint combination across all three protocols
- [ ] Staked-send + Jito dual submission tested end-to-end on devnet / small mainnet size before real capital is committed
- [ ] Treasury wallet funded above `treasury.min_balance_sol`, separate from any liquidation-principal path (there is none — principal is always flash-borrowed)
- [ ] Tip-curve constant `k` and per-reserve contention map calibrated from `observe`-mode data before promotion to `live` (see [`STRATEGY.md`](./STRATEGY.md#cold-start))
- [ ] All four circuit breakers (consecutive-revert, treasury floor, drawdown, sync lag) verified to actually halt submission in a controlled test, not just configured

## Incident response

| Signal | Meaning | Action |
|---|---|---|
| Sync-lag alert on one adapter | Account-sync pipeline drifting for that protocol | Adapter auto-halts; investigate Geyser feed health before re-arming |
| Repeated reverts on a route | Unwarmed ALT or optimistic CU estimate | Pull the route from the fire list; re-profile CU and re-warm its ALT |
| Landing rate collapse, all protocols | Submission-path or leader-schedule issue | Verify staked-send endpoint and Jito region; check leader schedule freshness |
| Losing races on one hot reserve | Write-lock contention spike | Rebuild the contention map; consider deprioritizing that reserve |
| Treasury-floor breaker fired | Hot wallet below `treasury.min_balance_sol` | Refill treasury; breaker auto-resets once above floor |
| Drawdown breaker fired | Realized 24h PnL below `risk.max_drawdown_usd` | Investigate before manual re-arm — do not reset without root-causing |

Staleness halts the affected protocol's adapter, not the whole system. Never let a stale sync pipeline keep submitting — every downstream decision would be computed against a world that no longer exists.

## Where this breaks

- **Market.** Blue-chip collateral on top pools is thin margin — better-capitalized searchers win most of those races. The realistic edge is isolated/newer markets, awkward-collateral routing generic bots skip, and cascade events where the latency hierarchy scrambles under load.
- **Technical.** The single point of failure is the LiteSVM account-sync pipeline. Instrument staleness detection specifically, alert on it, and halt the affected adapter on lag.
- **Execution.** The costliest failure is a transaction that passes simulation but does not fit on-chain because of an unwarmed ALT or an optimistic CU estimate — a guaranteed revert paid for in priority fees. Warmed ALTs and tight per-route CU limits are the difference between losing money on every attempt and running in production.
