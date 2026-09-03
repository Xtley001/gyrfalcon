# 02_ROUTING_DEX.md
### Owns: which DEX venues seized collateral is swapped through, verified program
### IDs, and route-selection rules. Does NOT own: flash-loan sourcing (→ 01_PROTOCOLS.md),
### CU/byte feasibility gating (→ 04_STRATEGY_RISK.md).

---

## 1. Verified program IDs (mainnet-beta)

| Venue | Program ID | Status |
|---|---|---|
| Raydium CLMM | `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK` | Already wired (`crates/bundler/src/swaps.rs`) |
| Raydium CP Swap | `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C` | Already wired |
| Orca Whirlpool | `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc` | Already wired |
| Meteora DLMM | `LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo` | Already wired |
| Phoenix (Ellipsis Labs) | `PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY` | **NEW — verified against Phoenix's own repo (`Ellipsis-Labs/phoenix-v1`) and `phoenix-cli` official releases** |
| Jupiter Aggregator v6 | `JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4` | **NEW — fallback only, see §3** |

**OpenBook v2:** not added to this table. Its current program ID needs
independent confirmation directly from `openbook-dex`'s official repository
before it goes into `dex_programs`, same standard as every row above —
do not carry it over from a memory of the older OpenBook v1/Serum ID, which
is a different, deprecated program. Treat as a candidate for a follow-up
session, not part of this build order.

## 2. Why Phoenix, why not just add more AMMs

Phoenix is a crankless on-chain **orderbook**, not an AMM. The four venues
already integrated are all AMM/CLMM-style — good for most pairs, but they
have slippage curves that get worse with size on any given pool. For your
highest-volume seized-collateral pairs (SOL/USDC, USDT/USDC, and any
JitoSOL/mSOL/other-LST pair, since LST collateral is common across all three
lending protocols), an orderbook can fill large size with less price impact
than walking an AMM curve. Route selection (§3) should try Phoenix first for
these specific pairs when its book has sufficient depth, falling back to the
AMM venues otherwise.

## 3. Route selection rule

1. For the seized-collateral mint → debt mint pair, check direct pool/market
   depth across all six venues in §1 in parallel (this is already an
   established pattern — `MultiSourceRouter` does the equivalent depth/fee
   comparison for flash sources in `01_PROTOCOLS.md`; the DEX router should
   follow the same shape).
2. Pick the venue with the best net price after fees for the sized amount —
   not the deepest venue in isolation, the best realized price at *this*
   trade size.
3. **Jupiter is fallback only**, used when none of the five direct venues
   have sufficient depth for the trade size at an acceptable price impact
   threshold (define the threshold in `04_STRATEGY_RISK.md`, don't hardcode
   it here). Going through Jupiter means one extra CPI hop and more CU spent
   versus a direct swap instruction — acceptable when it's the only route
   that avoids unacceptable slippage, not acceptable as a default because
   it's easier to integrate.
4. Every route decision is logged with: venue chosen, quoted price, realized
   price (post-fill, from the simulation result), and which other venues
   were considered and rejected with their quoted price. This is what lets
   you audit whether route selection is actually picking the best venue
   over time, not just working.

## 4. Testable output

- Unit test: given synthetic pool/market states for all six venues at known
  depths, `select_route()` returns the venue with the best net price for a
  given trade size — not necessarily the deepest one.
- Integration test via `crates/sim`: a simulated liquidation with collateral
  in a thin AMM pool and a deep Phoenix market picks Phoenix.
- Replay: run the existing `tests/fixtures/liquidations.jsonl` set through
  the new router; confirm no regression in route choice for pairs that were
  already correctly routed to Raydium/Orca/Meteora.
