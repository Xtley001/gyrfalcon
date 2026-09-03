# 05_DASHBOARD.md
### Owns: what the dashboard shows and where its data comes from. Does NOT own:
### the underlying data itself — `LiquidationLog` and `PnlLedger` are the source
### of truth (per `crates/store/src/liquidation_log.rs`'s own doc comment: "the
### dashboard is a visual reference, not a data API" — do not reverse that).

---

## 1. Current state

`dashboard.html` (385 lines, static) has two sections today: "Recent
liquidations" and "Adapter health." Source data already exists and is richer
than what's surfaced: `LiquidationLog` (append-only terminal outcomes,
JSONL-backed) and `PnlLedger` (rolling realized PnL by slot window, already
feeding the drawdown breaker).

## 2. Required new panels

| Panel | Data source | Fields |
|---|---|---|
| **Live PnL** | `PnlLedger` | Realized PnL over the same rolling window the drawdown breaker uses — must match `04_STRATEGY_RISK.md`'s derived `max_drawdown_usd` window exactly, not a separately-chosen display window |
| **Per-protocol breakdown** | `LiquidationLog` | Count + net USD, split by Kamino / Save / MarginFi / Drift (once added per `01_PROTOCOLS.md`) |
| **Per-venue swap breakdown** | Route-selection log (`02_ROUTING_DEX.md §3`) | Which of the 6 venues executed each swap leg, and realized vs. quoted price delta |
| **Submission path outcomes** | `crates/submit` outcome events | Landed via Staked QUIC vs. Jito vs. missed, per `03_SUBMISSION_LATENCY.md`'s race semantics |
| **Circuit breaker state** | `breakers.rs` | Current tier, which threshold (if any) is currently tripped, time since last trip |
| **Latency histogram** | Timestamp breach-detected → timestamp landed | p50/p90/p99, split by submission path |
| **Flash-source usage** | `MultiSourceRouter` | Which source was selected per liquidation, and how often Save/MarginFi were *skipped* due to the unverified-flash-loan gate in `01_PROTOCOLS.md §2` — this number should trend toward zero as those get verified |

## 3. Data contract rule

Every panel reads from `LiquidationLog`, `PnlLedger`, or a new equivalently
append-only/queryable store — never by scraping or re-deriving from the
dashboard's own prior rendered output, and never by the dashboard computing
business logic (PnL math, breach eligibility, etc.) itself. The dashboard
renders what the engine already computed; it does not recompute it
differently, or the two will drift.

## 4. Refresh mechanism

Specify explicitly rather than leaving it to be invented: is this a
server-pushed live feed (the orchestrator daemon already runs an "embedded
dashboard server" per the README's crate table — confirm whether it currently
does anything beyond serving the static file), a polling interval, or a
manual refresh? Given the latency focus of this whole system, a static
polling dashboard undersells the engine underneath it — server-push
(WebSocket or SSE from the existing embedded server) is the right target so
the latency histogram and live PnL panels are actually live, not stale by a
poll interval. This is a decision to confirm before building, not to assume.

## 5. Testable output

- Every new panel has a corresponding fixture in `tests/fixtures/` (or a new
  one) that produces known input, and the rendered dashboard shows the
  matching expected values.
- Per-protocol and per-venue breakdowns sum to the same total as the
  existing "Recent liquidations" count — a coverage check, not just a
  rendering check.
