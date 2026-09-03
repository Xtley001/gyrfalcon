# POST_BUILD_AUDIT_PROMPT.md
### Run this at the end of every step in 06_BUILD_ORDER.md, and again as a
### full-system pass once all 10 steps are done. This is a separate session
### from the build session — an implementer auditing its own unverified claims
### in the same context it wrote them in is the weakest possible check.

---

```
Do not trust the summary from the previous session. Verify everything in
this codebase from scratch against the MD set (00_SYSTEM_CONTEXT.md through
06_BUILD_ORDER.md) and against DECISIONS.md. Where a claim was made that
something "works" or "passes," re-run it yourself and show the actual
output — do not accept a prior claim as evidence.

Structure your answer as a pass/fail table, one row per item below. For
every FAIL, state exactly what's wrong and which MD it violates. For every
PASS, state the actual evidence (command run + output, or file + line) —
"looks correct" is not evidence.

## A. Build integrity
1. `cargo build --workspace` — paste the actual output. Zero warnings
   treated as informational only if you list them; zero errors is
   mandatory.
2. `cargo test --workspace` — paste the actual output, full pass count.
   Any skipped or ignored test: name it and say why it's ignored.
3. `cargo run --bin replay -- --events tests/fixtures/liquidations.jsonl`
   — paste output. Compare candidate outcomes against what the same replay
   produced before this round of changes, if that baseline is available.

## B. Address verification (flash-loan and swap correctness depends on this
   being exact — a wrong program ID here is not a cosmetic bug)
4. Every program ID in the codebase's router, health adapters, and
   bundler — list them all, and for each one, confirm it matches
   01_PROTOCOLS.md or 02_ROUTING_DEX.md exactly, byte for byte. Flag any
   address in the code that does NOT appear in either MD — that's either
   an undocumented addition or a typo, and both are critical.
5. Any address in the MDs marked "NOT VERIFIED" (e.g. OpenBook v2) — confirm
   it was NOT wired into live code. If it was, that's a FAIL regardless of
   whether it happens to work.

## C. Flash-loan path correctness — the highest-stakes category, treat it
   accordingly
6. For each of Kamino, Save, MarginFi: is the flash-loan path gated exactly
   as 01_PROTOCOLS.md §2 specifies? Specifically — can the router select
   Save or MarginFi as a live flash source WITHOUT a corresponding verified
   transaction signature logged in DECISIONS.md? Show the actual gating
   code, not a description of it.
7. If Save or MarginFi flash-loan verification transactions exist in
   DECISIONS.md, pull the signature and confirm — via chain lookup if you
   have that capability, or by describing exactly what would need to be
   checked if you don't — that it's a real, landed, non-devnet-only
   verification if the system is intended to go live on mainnet.
8. Route-feasibility stepping (04_STRATEGY_RISK.md §2): show a test case
   where the naive close-factor size would be infeasible, and confirm the
   actual code steps it down rather than either failing silently or
   attempting an infeasible bundle. Run it, don't describe it.
9. Confirm the repay amount computed by sizing logic matches, exactly,
   what the flash-loan instruction being built actually requests and what
   the repay instruction actually repays. A mismatch between "amount
   sized" and "amount in the instruction" is the single most dangerous
   class of bug in this entire system — trace it through the actual
   instruction-building code, line by line, don't infer it from function
   names.

## D. Risk parameters
10. Open the live `config/gyrfalcon.toml` (not the example file). For every
    `[risk]` and `[treasury]` value, confirm it has a corresponding
    derivation entry in DECISIONS.md per 04_STRATEGY_RISK.md §3. Any value
    that still matches the placeholder example file with no derivation
    note is a FAIL.
11. Circuit breakers (breakers.rs): confirm the 4 tiers still exist,
    unmodified in shape, with only thresholds changed — flag if tiers were
    added or removed, since that wasn't in scope per 04_STRATEGY_RISK.md §4.

## E. Submission and latency
12. Jito tip accounts: confirm the live `getTipAccounts` call path exists
    and the hardcoded list in 03_SUBMISSION_LATENCY.md §2 is the fallback
    only, not the primary source. Show the code.
13. Confirm no tip account is referenced inside an Address Lookup Table
    anywhere in the bundler — search for it explicitly, don't assume.
14. Leader-awareness gating before the Jito submission leg: show the actual
    check, and confirm there's a test proving the Jito leg is skipped when
    no upcoming leader is Jito-Solana.
15. Dual-path race: confirm there is no code path that can produce two
    recorded landed outcomes for the same liquidation (duplicate entries in
    LiquidationLog from both submission paths succeeding).

## F. Dashboard
16. Every panel in 05_DASHBOARD.md §2 — confirm each reads from
    LiquidationLog/PnlLedger/the route-selection log, not from a
    recomputation of business logic inside the dashboard code itself.

## G. Scope discipline
17. `git diff` (or equivalent) against the pre-session state — list every
    file touched. Flag anything touched that isn't accounted for by the
    step(s) being audited. Unexplained touched files are a FAIL even if the
    change itself looks harmless.

## Final verdict
After all rows: state plainly whether this is ready for `--mode observe`
(zero capital at risk) versus `--mode live`. Do not recommend `--mode live`
unless every item in section C passed with real evidence, not inference.
If anything in this audit could not actually be verified (no chain access,
no way to run a command in this environment, etc.), say so explicitly
instead of marking it PASS by default — an unverifiable item is neither a
pass nor a fail, it's an open item, and it should be listed as one.
```
