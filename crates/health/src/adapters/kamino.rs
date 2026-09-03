//! Kamino `HealthAdapter` — decodes real `Reserve`, `Obligation`, and
//! `LendingMarket` accounts from the deployed Kamino klend program
//! (`KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD` on mainnet).
//!
//! # Where the account layouts come from
//!
//! This adapter decodes against [`klend-interface`](https://crates.io/crates/klend-interface),
//! Kamino's own maintained, off-chain interface crate — not a hand-transcribed
//! or guessed copy of their on-chain program's struct layout. That distinction
//! matters twice over:
//!
//! - **Correctness.** `docs/ARCHITECTURE.md` calls a stale/wrong account
//!   layout dangerous specifically because it fails silently — a
//!   miscalculated health factor doesn't panic, it just produces a wrong
//!   number. Depending on Kamino's own published crate instead of
//!   re-deriving the byte layout by hand removes an entire class of
//!   transcription risk from that surface.
//! - **Licensing.** The klend program itself is BUSL-1.1 licensed with
//!   *no* production-use grant (Kamino's on-chain program source is
//!   licensed for non-production use only, absent a commercial license).
//!   `klend-interface` is a **separate** crate under its own BUSL-1.1 terms
//!   whose Additional Use Grant explicitly permits exactly this: "You may
//!   use the Licensed Work, in unmodified form, as a dependency in your own
//!   applications." This adapter does exactly that — an ordinary
//!   `Cargo.toml` dependency, unmodified, never vendored or copied into
//!   this repo — and does not use the program crate itself.
//!
//! # What this adapter does *not* do (yet)
//!
//! The on-chain `Obligation` account already carries Kamino's own computed
//! risk figures (`borrow_factor_adjusted_debt_value_sf`,
//! `unhealthy_borrow_value_sf`) — refreshed by Kamino's `refresh_obligation`
//! instruction, which some other transaction in the same slot must have
//! called recently for these to be current. This adapter reads those
//! figures rather than recomputing LTV from oracle prices itself; it does
//! **not** independently verify staleness beyond `LastUpdate`'s slot, and
//! does not yet re-run `refresh_reserve`/`refresh_obligation` logic
//! itself. That gap, plus real historical validation, is exactly what
//! Stage B item 5 (the replay harness) and item 12 (the mainnet `observe`
//! run) exist to close — see `docs/BUILD_ORDER.md`. Nothing here should be
//! treated as validated until the replay harness's exit criteria pass.
//!
//! `close_factor_max_repay`'s conversion from Kamino's USD-scale
//! "market value" figures back to native token units
//! ([`max_repay_native_amount`]) is a straightforward reimplementation of
//! the publicly-documented, plain config values (`liquidation_max_debt_close_factor_pct`,
//! `max_liquidatable_debt_market_value_at_once` — ordinary `u8`/`u64`
//! fields, not an algorithm), done in `f64` rather than Kamino's on-chain
//! fixed-point type. It does not replicate Kamino's on-chain rounding
//! behavior exactly, which is one more reason this number is a sizing
//! *input* for Stage B's `strategy` crate to validate against the replay
//! set, not a number to trust standalone.

use gyrfalcon_core::traits::AccountUpdate;
use gyrfalcon_core::types::BreachCandidate;
use gyrfalcon_core::{HealthAdapter, Protocol, Pubkey as CorePubkey};
use klend_interface::state::{
    from_account_data, LendingMarket, Obligation, Reserve, SplDiscriminate,
};
use std::collections::HashMap;

/// The deployed Kamino klend program on mainnet.
fn kamino_program_id() -> CorePubkey {
    to_core_pubkey(klend_interface::KLEND_PROGRAM_ID)
}

fn to_core_pubkey(p: solana_pubkey::Pubkey) -> CorePubkey {
    CorePubkey::new(p.to_bytes())
}

/// Fixed-point `_sf` field -> `f64`. See the module doc for why `f64` (not
/// chained `Fraction` arithmetic) is used for this adapter's own math.
fn sf_to_f64(sf: u128) -> f64 {
    klend_interface::Fraction::from_bits(sf).to_num::<f64>()
}

#[derive(Debug, Clone, Copy)]
struct ReserveSnapshot {
    liquidity_mint: CorePubkey,
    market_price_sf: u128,
    mint_decimals: u32,
    slot: u64,
    /// Unix timestamp (not slot — Kamino stores this field in wall-clock
    /// seconds) the oracle price was last refreshed, and the reserve's own
    /// configured staleness ceiling. Both needed for
    /// [`KaminoAdapter::reserve_price_is_stale`].
    market_price_last_updated_ts: u64,
    max_age_price_seconds: u64,
}

#[derive(Debug, Clone, Copy)]
struct LendingMarketSnapshot {
    liquidation_max_debt_close_factor_pct: u8,
    max_liquidatable_debt_market_value_at_once: u64,
    slot: u64,
}

#[derive(Debug, Clone, Copy)]
struct ObligationSnapshot {
    slot: u64,
    close_factor_max_repay: u64,
}

/// `gyrfalcon_core::HealthAdapter` for Kamino.
///
/// Tracks the `Reserve` and `LendingMarket` accounts it has seen so it can
/// resolve an `Obligation`'s liquidity/collateral reserves to mints and
/// prices; an `Obligation` update referencing a reserve this adapter
/// hasn't observed yet is skipped for this update (not an error — the
/// reserve account will typically already have been seen, since Kamino's
/// own `refresh_obligation` touches it in the same or a preceding slot,
/// but this adapter makes no assumption about update ordering).
#[derive(Debug, Default)]
pub struct KaminoAdapter {
    reserves: HashMap<CorePubkey, ReserveSnapshot>,
    lending_markets: HashMap<CorePubkey, LendingMarketSnapshot>,
    /// Position count for the dashboard stat band — every obligation this
    /// adapter has decoded at least once, whether currently breached or not.
    known_obligations: HashMap<CorePubkey, ObligationSnapshot>,
    last_synced_slot: u64,
    current_slot: u64,
}

impl KaminoAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    fn handle_reserve(&mut self, pubkey: CorePubkey, data: &[u8], slot: u64) {
        if let Ok(reserve) = from_account_data::<Reserve>(data) {
            self.reserves.insert(
                pubkey,
                ReserveSnapshot {
                    liquidity_mint: to_core_pubkey(reserve.liquidity.mint_pubkey),
                    market_price_sf: reserve.market_price(),
                    mint_decimals: reserve.mint_decimals() as u32,
                    slot,
                    market_price_last_updated_ts: reserve.liquidity.market_price_last_updated_ts,
                    max_age_price_seconds: reserve.config.token_info.max_age_price_seconds,
                },
            );
        }
    }

    /// Oracle price staleness for a given reserve, per
    /// `docs/RUNBOOK.md`'s production readiness checklist item "Oracle
    /// staleness/confidence handling verified per protocol."
    ///
    /// **Not part of `gyrfalcon_core::HealthAdapter`** — that trait's
    /// `on_account_update` only carries a slot, and Kamino's staleness
    /// field is a Unix wall-clock timestamp
    /// (`liquidity.market_price_last_updated_ts`), not a slot count, so
    /// checking it needs the caller to supply the current wall-clock time
    /// separately. This is an inherent method a caller (the arbitration
    /// step in `strategy`, once wired up) should call before trusting a
    /// `BreachCandidate` this adapter produced — sizing/routing off a
    /// stale oracle price is exactly the kind of silently-wrong-number
    /// failure `docs/ARCHITECTURE.md` warns about.
    ///
    /// Returns `None` if this reserve hasn't been observed yet (nothing
    /// to judge staleness against).
    pub fn reserve_price_is_stale(
        &self,
        reserve: CorePubkey,
        current_unix_ts: u64,
    ) -> Option<bool> {
        let snapshot = self.reserves.get(&reserve)?;
        let age = current_unix_ts.saturating_sub(snapshot.market_price_last_updated_ts);
        Some(age > snapshot.max_age_price_seconds)
    }

    fn handle_lending_market(&mut self, pubkey: CorePubkey, data: &[u8], slot: u64) {
        if let Ok(market) = from_account_data::<LendingMarket>(data) {
            self.lending_markets.insert(
                pubkey,
                LendingMarketSnapshot {
                    liquidation_max_debt_close_factor_pct: market
                        .liquidation_max_debt_close_factor_pct,
                    max_liquidatable_debt_market_value_at_once: market
                        .max_liquidatable_debt_market_value_at_once,
                    slot,
                },
            );
        }
    }

    fn handle_obligation(
        &mut self,
        pubkey: CorePubkey,
        data: &[u8],
        slot: u64,
    ) -> Option<BreachCandidate> {
        let obligation = from_account_data::<Obligation>(data).ok()?;

        let debt = sf_to_f64(obligation.borrow_factor_adjusted_debt_value());
        if debt <= 0.0 {
            self.known_obligations.insert(pubkey, ObligationSnapshot { slot, close_factor_max_repay: 0 });
            return None; // no debt outstanding, cannot be liquidated
        }
        let unhealthy = sf_to_f64(obligation.unhealthy_borrow_value());
        if unhealthy <= 0.0 {
            self.known_obligations.insert(pubkey, ObligationSnapshot { slot, close_factor_max_repay: 0 });
            return None; // zero collateral deposited, cannot seize anything
        }
        let health_factor = unhealthy / debt;
        if health_factor >= 1.0 {
            self.known_obligations.insert(pubkey, ObligationSnapshot { slot, close_factor_max_repay: 0 });
            return None; // healthy
        }

        let market = to_core_pubkey(obligation.lending_market);
        let lending_market = self.lending_markets.get(&market)?;

        let liquidity = obligation
            .borrows
            .iter()
            .filter(|b| b.is_active())
            .max_by(|a, b| a.market_value().cmp(&b.market_value()))?;
        let debt_reserve_key = to_core_pubkey(liquidity.borrow_reserve);
        let debt_reserve = self.reserves.get(&debt_reserve_key)?;

        let collateral = obligation
            .deposits
            .iter()
            .filter(|d| d.is_active())
            .max_by(|a, b| a.market_value().cmp(&b.market_value()))?;
        let collateral_reserve_key = to_core_pubkey(collateral.deposit_reserve);
        let collateral_reserve = self.reserves.get(&collateral_reserve_key)?;

        let close_factor_max_repay =
            max_repay_native_amount(lending_market, debt_reserve, liquidity);

        self.known_obligations.insert(
            pubkey,
            ObligationSnapshot {
                slot,
                close_factor_max_repay,
            },
        );

        Some(BreachCandidate {
            protocol: Protocol::Kamino,
            position_id: pubkey,
            collateral_mint: collateral_reserve.liquidity_mint,
            debt_mint: debt_reserve.liquidity_mint,
            health_factor,
            close_factor_max_repay,
            slot,
        })
    }
}

/// Maximum native-token repay amount for one obligation liquidity position,
/// per the module doc's caveats. Returns `0` rather than erroring on a
/// zero/degenerate price so a bad oracle read produces an unprofitable
/// (and therefore discarded) candidate rather than a panic.
fn max_repay_native_amount(
    market: &LendingMarketSnapshot,
    debt_reserve: &ReserveSnapshot,
    liquidity: &klend_interface::state::ObligationLiquidity,
) -> u64 {
    let debt_mv = sf_to_f64(liquidity.market_value());
    let close_factor_rate = market.liquidation_max_debt_close_factor_pct as f64 / 100.0;
    let by_close_factor_mv = debt_mv * close_factor_rate;
    let market_cap_mv = market.max_liquidatable_debt_market_value_at_once as f64;
    let capped_mv = by_close_factor_mv.min(market_cap_mv);

    let price = sf_to_f64(debt_reserve.market_price_sf);
    if price <= 0.0 || !price.is_finite() {
        return 0;
    }

    let native = capped_mv / price * 10f64.powi(debt_reserve.mint_decimals as i32);
    let borrowed_native = sf_to_f64(liquidity.borrowed_amount());
    native.min(borrowed_native).max(0.0) as u64
}

impl HealthAdapter for KaminoAdapter {
    fn on_account_update(&mut self, update: AccountUpdate) -> Option<BreachCandidate> {
        if update.owner != kamino_program_id() {
            return None;
        }
        self.current_slot = self.current_slot.max(update.slot);

        let data = &update.data;
        if data.len() < 8 {
            return None;
        }
        let discriminator = &data[..8];

        if discriminator == Reserve::SPL_DISCRIMINATOR_SLICE {
            self.handle_reserve(update.pubkey, data, update.slot);
            self.last_synced_slot = update.slot;
            None
        } else if discriminator == LendingMarket::SPL_DISCRIMINATOR_SLICE {
            self.handle_lending_market(update.pubkey, data, update.slot);
            self.last_synced_slot = update.slot;
            None
        } else if discriminator == Obligation::SPL_DISCRIMINATOR_SLICE {
            self.last_synced_slot = update.slot;
            self.handle_obligation(update.pubkey, data, update.slot)
        } else {
            None
        }
    }

    fn close_factor(&self, position_id: CorePubkey) -> u64 {
        // Only meaningful once `on_account_update` has decoded this
        // obligation at least once with all its reserves resolvable;
        // otherwise there's nothing to compute a close factor from.
        self.known_obligations
            .get(&position_id)
            .map(|s| s.close_factor_max_repay)
            .unwrap_or(0)
    }

    fn position_count(&self) -> usize {
        self.known_obligations.len()
    }

    fn sync_lag_slots(&self) -> u64 {
        self.current_slot.saturating_sub(self.last_synced_slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;
    use klend_interface::state::{
        LendingMarket as KLendingMarket, Obligation as KObligation, ObligationCollateral,
        ObligationLiquidity, Reserve as KReserve,
    };

    // Kamino's `_sf` fixed-point fields are `U68F60`: 1.0 == 2^60. See
    // klend_interface::Fraction's module doc.
    const FRACTION_ONE: u128 = 1u128 << 60;

    /// Encodes a plain f64 (a USD market value OR a raw native token
    /// amount — both `_sf` fields share the same Q68.60 encoding, only
    /// the caller's interpretation differs) as a `_sf`-style fixed-point
    /// u128.
    fn to_sf(value: f64) -> u128 {
        (value * FRACTION_ONE as f64) as u128
    }

    fn encode_account<T: bytemuck::Pod + SplDiscriminate>(value: &T) -> Vec<u8> {
        let mut out = T::SPL_DISCRIMINATOR_SLICE.to_vec();
        out.extend_from_slice(bytemuck::bytes_of(value));
        out
    }

    fn program_owner() -> CorePubkey {
        kamino_program_id()
    }

    fn pk(byte: u8) -> solana_pubkey::Pubkey {
        solana_pubkey::Pubkey::new_from_array([byte; 32])
    }

    fn core_pk(byte: u8) -> CorePubkey {
        CorePubkey::new([byte; 32])
    }

    fn sample_reserve(mint: solana_pubkey::Pubkey, price_usd: f64, decimals: u64) -> KReserve {
        let mut reserve = KReserve::zeroed();
        reserve.liquidity.mint_pubkey = mint;
        reserve.liquidity.market_price_sf = to_sf(price_usd).into();
        reserve.liquidity.mint_decimals = decimals;
        reserve.liquidity.market_price_last_updated_ts = 1_000_000;
        reserve.config.token_info.max_age_price_seconds = 60;
        reserve
    }

    fn sample_lending_market(close_factor_pct: u8, cap_usd: u64) -> KLendingMarket {
        let mut market = KLendingMarket::zeroed();
        market.liquidation_max_debt_close_factor_pct = close_factor_pct;
        market.max_liquidatable_debt_market_value_at_once = cap_usd;
        market
    }

    /// An obligation with one active borrow (undercollateralized: debt
    /// exceeds the unhealthy threshold) and one active deposit.
    fn breached_obligation(
        lending_market: solana_pubkey::Pubkey,
        debt_reserve: solana_pubkey::Pubkey,
        collateral_reserve: solana_pubkey::Pubkey,
        debt_usd: f64,
        unhealthy_usd: f64,
        borrowed_native: u64,
    ) -> KObligation {
        let mut obligation = KObligation::zeroed();
        obligation.lending_market = lending_market;
        obligation.borrow_factor_adjusted_debt_value_sf = to_sf(debt_usd).into();
        obligation.unhealthy_borrow_value_sf = to_sf(unhealthy_usd).into();

        let mut liquidity = ObligationLiquidity::zeroed();
        liquidity.borrow_reserve = debt_reserve;
        liquidity.market_value_sf = to_sf(debt_usd).into();
        liquidity.borrowed_amount_sf = to_sf(borrowed_native as f64).into();
        obligation.borrows[0] = liquidity;

        let mut collateral = ObligationCollateral::zeroed();
        collateral.deposit_reserve = collateral_reserve;
        collateral.market_value_sf = to_sf(debt_usd * 1.1).into();
        obligation.deposits[0] = collateral;

        obligation
    }

    #[test]
    fn healthy_obligation_produces_no_candidate() {
        let mut adapter = KaminoAdapter::new();
        let market_key = pk(1);
        let debt_reserve_key = pk(2);
        let collateral_reserve_key = pk(3);

        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: encode_account(&sample_lending_market(20, 500_000)),
            slot: 100,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(2),
            owner: program_owner(),
            data: encode_account(&sample_reserve(pk(9), 1.0, 6)),
            slot: 100,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(3),
            owner: program_owner(),
            data: encode_account(&sample_reserve(pk(8), 1.0, 6)),
            slot: 100,
        });

        // Healthy: debt well below the unhealthy threshold.
        let obligation = breached_obligation(
            market_key,
            debt_reserve_key,
            collateral_reserve_key,
            500.0,
            1_000.0,
            500_000_000,
        );
        let result = adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(42),
            owner: program_owner(),
            data: encode_account(&obligation),
            slot: 101,
        });
        assert!(result.is_none());
    }

    #[test]
    fn breached_obligation_produces_candidate_with_correct_close_factor() {
        let mut adapter = KaminoAdapter::new();
        let market_key = pk(1);
        let debt_reserve_key = pk(2);
        let collateral_reserve_key = pk(3);
        let debt_mint = pk(9);
        let collateral_mint = pk(8);

        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: encode_account(&sample_lending_market(20, 500_000)), // 20% close factor, $500k cap
            slot: 200,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(2),
            owner: program_owner(),
            data: encode_account(&sample_reserve(debt_mint, 1.0, 6)), // $1 stablecoin, 6 decimals
            slot: 200,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(3),
            owner: program_owner(),
            data: encode_account(&sample_reserve(collateral_mint, 100.0, 9)),
            slot: 200,
        });

        // Breached: $1,200 debt against a $1,000 unhealthy threshold.
        // Close factor: min(20% * $1,200, $500,000 cap) = $240 -> 240_000_000 native units at $1/6dp.
        let obligation = breached_obligation(
            market_key,
            debt_reserve_key,
            collateral_reserve_key,
            1_200.0,
            1_000.0,
            1_200_000_000, // 1,200 tokens at 6dp, more than enough to cover the close-factor repay
        );
        let candidate = adapter
            .on_account_update(AccountUpdate {
                pubkey: core_pk(42),
                owner: program_owner(),
                data: encode_account(&obligation),
                slot: 201,
            })
            .expect("undercollateralized obligation should breach");

        assert_eq!(candidate.protocol, Protocol::Kamino);
        assert!(candidate.health_factor < 1.0);
        assert!((candidate.health_factor - (1_000.0 / 1_200.0)).abs() < 1e-9);
        assert_eq!(candidate.debt_mint, to_core_pubkey(debt_mint));
        assert_eq!(candidate.collateral_mint, to_core_pubkey(collateral_mint));

        let expected_repay = 240_000_000u64; // $240 worth of a $1, 6-decimal token
        let tolerance = expected_repay / 1000; // f64 rounding
        assert!(
            candidate.close_factor_max_repay.abs_diff(expected_repay) <= tolerance,
            "expected ~{expected_repay}, got {}",
            candidate.close_factor_max_repay
        );
    }

    #[test]
    fn close_factor_repay_is_capped_by_market_wide_ceiling_not_just_percentage() {
        let mut adapter = KaminoAdapter::new();
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: encode_account(&sample_lending_market(20, 100)), // tiny $100 market-wide cap
            slot: 300,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(2),
            owner: program_owner(),
            data: encode_account(&sample_reserve(pk(9), 1.0, 6)),
            slot: 300,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(3),
            owner: program_owner(),
            data: encode_account(&sample_reserve(pk(8), 100.0, 9)),
            slot: 300,
        });

        // 20% of $1,000,000 debt would be $200,000 -- far more than the $100 cap.
        let obligation = breached_obligation(pk(1), pk(2), pk(3), 1_000_000.0, 1.0, 5_000_000_000);
        let candidate = adapter
            .on_account_update(AccountUpdate {
                pubkey: core_pk(99),
                owner: program_owner(),
                data: encode_account(&obligation),
                slot: 301,
            })
            .expect("should breach");

        // Capped at $100 -> 100_000_000 native units at $1/6dp, not 20% of debt.
        assert!(candidate.close_factor_max_repay <= 100_500_000);
    }

    #[test]
    fn non_kamino_owner_is_ignored() {
        let mut adapter = KaminoAdapter::new();
        let result = adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: core_pk(255), // not the klend program
            data: vec![0u8; 64],
            slot: 1,
        });
        assert!(result.is_none());
        assert_eq!(adapter.position_count(), 0);
    }

    #[test]
    fn sync_lag_tracks_slots_since_last_klend_account_seen() {
        let mut adapter = KaminoAdapter::new();
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: encode_account(&sample_lending_market(20, 500_000)),
            slot: 1_000,
        });
        assert_eq!(adapter.sync_lag_slots(), 0);
    }

    #[test]
    fn reserve_price_is_stale_within_max_age_window() {
        let mut adapter = KaminoAdapter::new();
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(2),
            owner: program_owner(),
            // last_updated_ts = 1_000_000, max_age = 60s (sample_reserve).
            data: encode_account(&sample_reserve(pk(9), 1.0, 6)),
            slot: 100,
        });

        // 30s later: within the 60s window.
        assert_eq!(
            adapter.reserve_price_is_stale(core_pk(2), 1_000_030),
            Some(false)
        );
        // 90s later: past the 60s window.
        assert_eq!(
            adapter.reserve_price_is_stale(core_pk(2), 1_000_090),
            Some(true)
        );
    }

    #[test]
    fn reserve_price_staleness_is_unknown_for_an_unobserved_reserve() {
        let adapter = KaminoAdapter::new();
        assert_eq!(adapter.reserve_price_is_stale(core_pk(99), 1_000_000), None);
    }
}
