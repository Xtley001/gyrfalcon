# 04_STRATEGY_RISK.md
### Owns: position sizing, tip arbitration, circuit breakers, and how every
### placeholder risk number gets replaced with an empirical one. Does NOT own:
### which protocol/venue is chosen (→ 01/02), how a bundle is delivered (→ 03).

---

## 1. Current state

`crates/strategy/` already has: `sizing.rs` (close-factor + flash-depth
capping), `dynamic_tip.rs` + `tip.rs` (bidding), `breakers.rs` (4-tier circuit
breaker), `arbitration.rs`, `treasury_check.rs`. This is real logic, not
stubs — the gap is (a) one missing sizing dimension and (b) every threshold
being a labeled placeholder rather than a derived value.

## 2. Route-feasibility stepping (the missing sizing dimension)

`size_position()` today: `close_factor_max_repay.min(flash_source_available)`.
Missing: whether the resulting instruction set (flash borrow + repay +
liquidate + swap, across however many accounts each of those touches) fits
Solana's transaction limits — 1232-byte serialized transaction, 1.4M CU
compute budget per transaction (confirm current CU ceiling against
`solana.com` docs at implementation time, limits have changed before).

**Required addition:** after the close-factor/flash-depth size is computed,
build the actual instruction set at that size (or a cheap proxy for its
byte/CU footprint) and check it against the transaction limits *before*
committing to that size. If it doesn't fit:
- First try: does using an ALT reduce account references enough to fit?
  (`crates/bundler/src/alt.rs`, `alt_manager.rs` already exist — this is
  wiring them into the sizing decision, not building them from scratch.)
- If still infeasible even with ALTs: step the repay size down and re-check,
  rather than either failing the candidate outright or attempting a bundle
  that simulation will reject.
- Log every step-down so tip arbitration and PnL analysis can see when
  feasibility, not close-factor or flash-depth, was the binding constraint.

**Testable output:** given a synthetic breach candidate whose full
close-factor repay would require an infeasible instruction count, confirm
`size_position()` returns a stepped-down amount that passes LiteSVM
simulation, not the original amount.

## 3. Tip arbitration — from placeholder to empirical

`config/gyrfalcon.example.toml [risk]` today: `max_tip_per_tx_usd = 150.0`,
`max_tip_per_slot_usd = 600.0`, `max_tip_pct_of_bonus = 0.4`,
`contention_ceiling = 0.8` — every one labeled PLACEHOLDER in the file
itself.

**Required process, not a required number** (the number is empirical, this
file specifies how you get it, not what it is):
1. Run `cargo run --bin replay -- --events tests/fixtures/liquidations.jsonl`
   plus your own accumulated observe-mode history once you have it.
2. For each historical breach event, compute: what tip would have been
   needed to win against the actual landed liquidator (if you can observe
   competing bundles/tips from chain data), and what the liquidation bonus
   was worth.
3. Set `max_tip_pct_of_bonus` from the empirical distribution of
   tip-needed-to-win as a fraction of bonus, not a round number — e.g. the
   80th-percentile tip fraction that would still have won, if that's the
   win rate you're targeting.
4. Re-derive quarterly or after any period of materially different
   contention (a new large competing bot, a change in average position
   size on a given protocol) — this is not a set-once value.

## 4. Circuit breakers — tune, don't invent new tiers

The 4-tier breaker in `breakers.rs` already covers the shape you need.
Placeholder values to replace with derived ones the same way as §3:
`sync_lag_halt_slots`, `contention_ceiling`, `max_drawdown_usd`,
`consecutive_revert_limit`. Do not add a 5th tier without a specific gap
identified in the existing four — more tiers is not automatically more
safety, it's more surface for a threshold to be silently wrong.

## 5. Maximizing what's already here (in priority order)

1. **Close the route-feasibility gap (§2)** — every attempted-but-infeasible
   candidate today is either a wasted sim cycle or, worse, a bundle that
   gets built and submitted only to fail, which can burn a tip for nothing.
   This is the highest-leverage fix before adding Drift or any new venue.
2. **Empirical tip arbitration (§3)** — the difference between winning and
   losing a contested liquidation is almost always the tip, not which venue
   you picked for the swap. This is where the real edge is.
3. **Save/MarginFi flash-loan live verification** (`01_PROTOCOLS.md §2`) —
   unlocks flash-source depth you're currently not actually able to use
   live, which directly caps how large a position you can size against.
4. Only after 1–3: new venues (Phoenix, Drift) add incremental coverage,
   not a step-change, because they widen the set of catchable breaches
   rather than improving your win rate on the breaches you already see.

## 6. Testable output

- Replay run before/after §2's fix shows fewer sim-rejected candidates for
  the same historical event set.
- Tip arbitration values in a new `config/gyrfalcon.toml` are traceable to a
  specific replay run's output, cited in `DECISIONS.md`, not copied from
  the example file.
