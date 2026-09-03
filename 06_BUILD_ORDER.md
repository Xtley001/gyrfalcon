# 06_BUILD_ORDER.md
### Owns: session sequencing. One step, one layer, one session, per
### CLAUDE_BUILD_MASTER.md §1.3/§4.1. Do not batch steps to "save sessions" —
### that's exactly the scope-creep failure mode the build master warns about.

---

Paste `00_SYSTEM_CONTEXT.md` at the start of every session below, plus only
the MD(s) listed for that step. Use the Session Opening Protocol from
`CLAUDE_BUILD_MASTER.md §4.1` verbatim.

## Step 1 — Route-feasibility stepping
**MDs:** `00_SYSTEM_CONTEXT.md`, `04_STRATEGY_RISK.md §2`
**Depends on:** nothing new — `crates/bundler/src/alt.rs`/`alt_manager.rs` and
`crates/sim` already exist.
**Deliverable:** `size_position()` (or a new function it calls) rejects/steps
down infeasible sizes per §2's spec, wired through ALT usage.
**Acceptance criteria:** the synthetic-infeasible-candidate test in
`04_STRATEGY_RISK.md §2` passes; existing sizing tests in `sizing.rs`
still pass unmodified.

## Step 2 — Save flash-loan live verification
**MDs:** `00_SYSTEM_CONTEXT.md`, `01_PROTOCOLS.md §2 (Save)`
**Depends on:** Step 1 not required, can run in parallel — different files.
**Deliverable:** a logged, signed test transaction (devnet or small mainnet
size) proving Save flash borrow+repay lands; router's flash-source
selection gated on this proof per the acceptance criteria in
`01_PROTOCOLS.md`.
**Acceptance criteria:** transaction signature recorded in `DECISIONS.md`;
`MultiSourceRouter` will select Save as a live flash source only after
this exists.

## Step 3 — MarginFi flash-loan live verification
**MDs:** `00_SYSTEM_CONTEXT.md`, `01_PROTOCOLS.md §2 (MarginFi)`
**Depends on:** nothing from Step 2 — independent verification.
**Deliverable/acceptance criteria:** same shape as Step 2, for MarginFi.

## Step 4 — Jito tip-account live refresh + leader-awareness gating
**MDs:** `00_SYSTEM_CONTEXT.md`, `03_SUBMISSION_LATENCY.md §2–3`
**Depends on:** nothing new.
**Deliverable:** `getTipAccounts` polling with fallback, leader-schedule
check before the Jito leg fires.
**Acceptance criteria:** both unit tests in `03_SUBMISSION_LATENCY.md §5`
pass.

## Step 5 — Empirical risk parameters
**MDs:** `00_SYSTEM_CONTEXT.md`, `04_STRATEGY_RISK.md §3–4`
**Depends on:** Step 1 (feasibility stepping should be in before you derive
tip parameters, since it changes which candidates even reach the tip
decision).
**Deliverable:** a real `config/gyrfalcon.toml` (not the example file) with
every `[risk]` value traced to a specific replay run, cited in
`DECISIONS.md`.
**Acceptance criteria:** none of the values match the placeholder example
file's numbers by coincidence-checking — each has a derivation note.

## Step 6 — Phoenix venue integration
**MDs:** `00_SYSTEM_CONTEXT.md`, `02_ROUTING_DEX.md §1–3`
**Depends on:** nothing new — additive to `crates/bundler/src/swaps.rs`.
**Deliverable:** Phoenix swap instruction generator + route-selection
inclusion.
**Acceptance criteria:** tests in `02_ROUTING_DEX.md §4`.

## Step 7 — Jupiter fallback routing
**MDs:** `00_SYSTEM_CONTEXT.md`, `02_ROUTING_DEX.md §3`
**Depends on:** Step 6 (route selection logic should already exist for the
five direct venues before adding the fallback branch).
**Deliverable/acceptance criteria:** per `02_ROUTING_DEX.md §3` item 3–4.

## Step 8 — Dashboard: data-layer panels
**MDs:** `00_SYSTEM_CONTEXT.md`, `05_DASHBOARD.md §2–3`
**Depends on:** Steps 1–7 producing the log fields the new panels read
(per-venue, per-path, feasibility-step-down counts). Do this step last for
that reason, not because dashboards are unimportant.
**Deliverable:** every panel in `05_DASHBOARD.md §2` backed by real data.
**Acceptance criteria:** fixture-driven tests per `05_DASHBOARD.md §5`.

## Step 9 — Dashboard: live refresh mechanism
**MDs:** `00_SYSTEM_CONTEXT.md`, `05_DASHBOARD.md §4`
**Depends on:** Step 8.
**Deliverable:** server-push (WebSocket/SSE) replacing static/poll, per the
confirmed decision from `05_DASHBOARD.md §4`.

## Step 10 — Drift Protocol integration
**MDs:** `00_SYSTEM_CONTEXT.md`, `01_PROTOCOLS.md §2 (Drift)`
**Depends on:** none of the above strictly, but do it last — it's the
largest single addition (new adapter, new liquidation mechanic, no shared
flash-loan path), and per `04_STRATEGY_RISK.md §5` it's lower-leverage than
fixing what's already built. Do not let "new protocol" excitement jump the
queue ahead of Steps 1–3, which fix real gaps in what you already have.
**Deliverable/acceptance criteria:** per `01_PROTOCOLS.md`'s Drift section.

---

## Session handoff template (use at the end of every step above)

```
# Session Handoff — Step [N]: [name]
Verified and working: [specifics — file, function, test name, not "it works"]
Stopped here: [exact file/function/line]
Decisions made this session: [→ DECISIONS.md]
MD updates needed: [which file, what changed]
Known issues: [anything working but not right]
Next session opens with: Step [N+1] per this file
```
