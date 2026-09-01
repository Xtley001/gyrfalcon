# Data Policy: No Mock or Synthetic Data

This is a hard rule, not a style preference: **no surface in `gyrfalcon` — dashboard, CLI output, logs, status page, or any future frontend — may ever display mock, synthetic, randomly jittered, or placeholder-dressed-as-real data.** Not during early development, not for "visual polish," not temporarily. Zero exceptions, including before the backend exists.

## Why this is absolute, not a preference

This system fires real transactions with real capital and gates every submission behind circuit breakers the operator has to trust at a glance (`docs/STRATEGY.md#circuit-breakers`). A dashboard that fabricates a plausible-looking number — even a harmlessly jittered slot counter for "ambient feel" — trains whoever is watching it to treat displayed numbers as decorative rather than diagnostic. The first time that habit meets a real treasury-floor breaker or a real drawdown number, the operator's instinct to trust the screen is exactly backwards. There is no safe amount of fake data on a control surface for a system like this.

## Scope

- `dashboard.html` and any future frontend.
- CLI output and structured logs presented as live.
- Any status page or public-facing monitor.
- Documentation screenshots — a screenshot used in a doc must be captured from a real, connected run (devnet is fine — it's real data on a lower-stakes network) never from a static mock.

## What is required instead

- **Explicit empty states.** No data yet is shown as no data — `—`, `NO DATA`, `AWAITING CONNECTION` — never a plausible-looking placeholder number standing in for a real one. See `dashboard.html`'s current state for the pattern: every field that would need a live backend to populate honestly shows an empty state instead of a fabricated value.
- **Loading skeletons, not loading numbers.** A shimmer or skeleton with no value is fine. A number that changes while "loading" implies a value exists when it doesn't.
- **Devnet and testnet data are real data.** Only fabricated numbers are disallowed — a dashboard connected to a live devnet run with $0.01 trades is compliant; a dashboard with a hardcoded `$9,412` is not, regardless of how small the real number would be.

## Enforcement in the build order

- Per [`BUILD_ORDER.md`](./BUILD_ORDER.md), the dashboard is not wired to fabricated data at any stage — it stays in its empty/disconnected state until `store::export_liquidation_log` and a real event feed exist to wire it to (`docs/API.md#state-store`), at which point it is wired to those, not before.
- Any PR that adds a hardcoded value to `dashboard.html` (or any future frontend file) and presents it as live should be treated by reviewers as a policy violation, not a style nit — see [`CONTRIBUTING.md`](../CONTRIBUTING.md).
