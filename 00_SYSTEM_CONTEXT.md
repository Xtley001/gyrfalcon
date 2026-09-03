# 00_SYSTEM_CONTEXT.md
### Owns: what this system is, what's already built, what's locked, what's explicitly out of scope.
### Every other MD references this file by name. Do not re-derive scope elsewhere.

---

## 1. What this is

`gyrfalcon` is a Solana-native (SVM-only) liquidation engine. It watches Kamino
Lend, Save, and MarginFi v2 obligations for health-factor breaches, sources a
flash loan, repays the bad debt, seizes collateral, swaps it back to the debt
asset, and repays the flash loan — all inside one atomic transaction bundle.

This MD set is a spec for **hardening and extending an existing, partially-built
codebase**, not a greenfield build. Read `crates/` before writing code against
any of these documents — if the code disagrees with an MD, the MD is wrong and
gets corrected, not the other way around.

## 2. Locked decisions (do not revisit without flagging it)

- **Chain: Solana mainnet-beta only.** No EVM chains, no bridges, no other SVM
  chain. As of this writing there is no second SVM chain with lending markets
  carrying real liquidation flow — do not add one speculatively. If that
  changes, it's a new decision, logged in `DECISIONS.md`, not a silent addition.
- **Language/stack is locked to what's in `Cargo.toml` today** — Rust workspace,
  `klend-interface`, `solend-sdk`, `marginfi-type-crate`, LiteSVM, Yellowstone
  gRPC, `solana-sdk`/`solana-program`. Do not introduce a second language, a
  second RPC client library, or a second simulation engine.
- **"Private mempool" = Jito Block Engine, not a literal mempool.** Solana has
  no public mempool for anyone to be "private" relative to — transactions go
  straight to the current leader. What exists, and what this system already
  partially wires up, is Jito's Block Engine: bundles submitted directly to
  Jito, simulated, auctioned, and forwarded only to Jito-Solana leaders,
  never broadcast publicly before landing. That is the closest real thing to
  "private mempool" on this chain, and it's covered in full in
  `03_SUBMISSION_LATENCY.md`. Nobody builds a second, different private
  mempool on top of this — Jito's bundle path *is* the private-orderflow path.
- **No result is guaranteed.** These documents specify correct, verifiable,
  low-latency infrastructure. They cannot guarantee profitability — that
  depends on competition from other searchers, market volatility (which
  drives how often positions breach), capital available for tip bidding, and
  execution quality. Anyone telling you otherwise is selling something.

## 3. Current build state (verified against the repo, not assumed)

**Done / present:**
- Yellowstone gRPC ingestion, zero-copy account decode, lock-free ring buffer
- Health adapters for Kamino, Save, MarginFi (breach detection)
- `MultiSourceRouter` — flash-source routing across Kamino + Save reserves,
  ranked by depth/fee (`crates/router/src/lib.rs`)
- Position sizing to close-factor + flash-depth (`crates/strategy/src/sizing.rs`)
- Dynamic tip bidding, 4-tier circuit breakers, treasury check
  (`crates/strategy/`)
- LiteSVM in-process simulation + CU profiling + replay engine (`crates/sim/`)
- Bundle builder: liquidation ix, ATA handling, ALT manager, DEX swap ix
  generators for Raydium (CLMM + CP), Orca Whirlpool, Meteora DLMM
  (`crates/bundler/`)
- Dual-path submit: Staked QUIC + Jito Block Engine (`crates/submit/`)
- Treasury floor monitor + PnL ledger + sweep (`crates/treasury/`)
- Position book + async JSONL liquidation log (`crates/store/`)
- `dashboard.html` — static page, two sections today: "Recent liquidations",
  "Adapter health"

**Explicitly flagged as unverified in the code itself — fix these before
adding anything new:**
- Save's flash-loan implementation is described by Save's own docs as
  functionally limited. The router ranks Save reserves as flash-loan
  candidates anyway. **Live-verify a real Save-sourced flash borrow/repay
  lands before capital depends on it.**
- MarginFi's flash-loan instruction path is unverified in this codebase
  (`crates/router/src/lib.rs` doc comment references this directly as "same
  spirit as MarginFi's flash-instruction finding").
- `size_position()` does not yet do route-feasibility stepping — it doesn't
  know if a sized repay amount actually fits a transaction's CU/byte budget
  before committing to it. See `04_STRATEGY_RISK.md §2`.
- Every value in `config/gyrfalcon.example.toml`'s `[risk]` and `[treasury]`
  blocks is a labeled placeholder, not a validated number.

## 4. Out of scope for this MD set

- Any EVM chain, any bridge, any cross-chain messaging layer
- Any lending protocol without confirmed live liquidation flow today
  (see `01_PROTOCOLS.md §4` for the ones considered and rejected, and why)
- A literal "private mempool" service — see §2 above
- Guaranteeing or projecting specific profit numbers

## 5. Operating rules for every session against this MD set

```
R1 — DO NOT INVENT. If a program ID, endpoint, field, or threshold is not in
     these documents, do not fabricate one. Stop and ask.
R2 — DO NOT ASSUME. Name every ambiguity instead of resolving it silently.
R3 — STACK IS LOCKED per §2. No substitutions.
R4 — Every address in these documents must be traceable to a cited, official
     source (protocol's own docs/repo). If you cannot verify an address,
     mark it NOT VERIFIED and do not wire it into the router or bundler.
R5 — NO UNINVITED CHANGES to code outside the current task's scope.
R6 — ONE MD, ONE SESSION where possible — see 06_BUILD_ORDER.md.
R7 — CONFIRM BEFORE BUILDING: state files touched + acceptance criteria first.
R8 — Flag technical problems before writing code that would ship them.
R9 — If two MDs conflict, stop and name the conflict — do not pick one.
R10 — Log any decision made mid-session that isn't in these MDs.
```

## 6. Document index

| File | Owns |
|---|---|
| `01_PROTOCOLS.md` | Lending protocols, verified program IDs, per-protocol readiness |
| `02_ROUTING_DEX.md` | DEX swap venues, verified program IDs, route selection |
| `03_SUBMISSION_LATENCY.md` | Jito bundle path, staked QUIC, tip accounts, colocation |
| `04_STRATEGY_RISK.md` | Sizing, tip arbitration, circuit breakers, parameter derivation |
| `05_DASHBOARD.md` | Dashboard data contract and required panels |
| `06_BUILD_ORDER.md` | Session-by-session sequencing and acceptance criteria |
