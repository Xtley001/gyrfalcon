//! Save `HealthAdapter` — decodes real `Reserve`, `Obligation`, and
//! `LendingMarket` accounts from the deployed Save lending program.
//!
//! # Save is Solend, rebranded
//!
//! Save's own docs (`docs.save.finance`) are titled "Save (formerly
//! Solend)" and link straight to `solendprotocol/solana-program-library`
//! on GitHub — same program, same account layouts, same
//! `So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo` mainnet program ID (from
//! `solend-sdk`'s own `declare_id!`). This adapter depends on
//! [`solend-sdk`](https://crates.io/crates/solend-sdk), Save/Solend's own
//! maintained SDK crate — Apache-2.0, no restricted-use grant needed,
//! unlike Kamino's BUSL-1.1 `klend-interface` (see
//! `crates/health/src/adapters/kamino.rs`'s module doc for that
//! distinction).
//!
//! # Decode mechanism differs from Kamino
//!
//! Save's accounts are **not** zero-copy — they use SPL's `Pack` trait
//! (`Reserve::unpack_from_slice` / `Obligation::unpack_from_slice`), a
//! Borsh-adjacent hand-rolled (de)serialization, not a `#[repr(C)]`
//! `bytemuck::Pod` cast. This adapter uses `solend-sdk`'s own `unpack`
//! implementation rather than reimplementing that parsing, for the same
//! reason `kamino.rs` depends on `klend-interface` instead of hand-coding
//! Kamino's byte layout: a wrong hand-rolled parse fails silently with a
//! wrong health factor, not a visible crash (`docs/ARCHITECTURE.md`).
//!
//! # A discovery worth flagging: the close-factor constants are vestigial
//!
//! `solend-sdk::state::reserve` defines `LIQUIDATION_CLOSE_FACTOR` (20%)
//! and `MAX_LIQUIDATABLE_VALUE_AT_ONCE` ($500,000) as public constants,
//! which would map naturally onto the same close-factor-based sizing this
//! codebase uses for Kamino. **They are not referenced anywhere in the
//! on-chain program's instruction processor** (verified by searching
//! `token-lending/program/src` for both identifiers — zero matches). The
//! processor's actual `calculate_liquidation` caps `amount_to_liquidate`
//! only by the position's full `borrowed_amount_wads` (a liquidator can
//! pass `u64::MAX` to request full liquidation) — there is no partial-close
//! restriction currently enforced on-chain. This adapter's
//! `close_factor_max_repay` reflects that: it is the position's full
//! outstanding borrow for the largest liquidity leg, not 20% of it. Sizing
//! the actual repay smaller than this (route feasibility, flash depth) is
//! `strategy`'s job, unchanged from the Kamino case.
//!
//! # Flash-loan caveat for the router
//!
//! Save's own docs (`docs.save.finance/developers/flash-loans`) describe
//! their flash-loan support as functionally limited pending further
//! reentrancy-safety work as of this writing — worth weighing before
//! `router` leans on Save reserves as a flash source at real capital
//! scale, even though `docs/BUILD_ORDER.md` item 13 calls for exactly
//! that. Not a reason to skip Stage C's router work, but a live-verify
//! item before `submit.mode = "live"`, same spirit as the MarginFi
//! flash-instruction check in `docs/BUILD_ORDER.md` item 14.

use gyrfalcon_core::traits::AccountUpdate;
use gyrfalcon_core::types::BreachCandidate;
use gyrfalcon_core::{HealthAdapter, Protocol, Pubkey as CorePubkey};
use solana_program::program_pack::Pack;
use solend_sdk::state::{LendingMarket, Obligation, Reserve};
use std::collections::HashMap;

/// Save's mainnet program — see the module doc for why this is the same
/// ID as Solend's.
fn save_program_id_bytes() -> [u8; 32] {
    solend_sdk::solend_mainnet::id().to_bytes()
}

fn save_program_id() -> CorePubkey {
    CorePubkey::new(save_program_id_bytes())
}

fn to_core_pubkey(p: solana_program::pubkey::Pubkey) -> CorePubkey {
    CorePubkey::new(p.to_bytes())
}

/// Save's `Decimal` type is a Wad (18 decimal fixed point, scaled by
/// `10^18`) stored as a `[u64; 3]`-backed 192-bit unsigned integer via the
/// `uint` crate. `to_scaled_val()` (provided by `solend-sdk`) gives the
/// raw `10^18`-scaled integer; dividing by `1e18` as `f64` mirrors the
/// same "adapter's own math in f64, not the protocol's exact fixed-point
/// type" tradeoff `kamino.rs` makes, for the same reason (see that
/// module's doc: not a routing-grade number standalone, a sizing input to
/// validate against the Stage C replay set).
fn decimal_to_f64(d: solend_sdk::math::Decimal) -> f64 {
    let scaled = d.to_scaled_val().unwrap_or(u128::MAX);
    scaled as f64 / 1e18
}

#[derive(Debug, Clone, Copy)]
struct ReserveSnapshot {
    liquidity_mint: CorePubkey,
    slot: u64,
    /// Save's own on-chain staleness verdict (`LastUpdate.stale`,
    /// `docs/RUNBOOK.md`'s "oracle staleness/confidence handling"
    /// checklist item) plus the slot it was last refreshed at, so
    /// [`SaveAdapter::reserve_price_is_stale`] can also apply
    /// `STALE_AFTER_SLOTS_ELAPSED` itself rather than trusting a flag that
    /// may not have been recomputed since this specific account update.
    last_update_slot: u64,
    last_update_stale_flag: bool,
}

/// Save/Solend's own constant: an update older than this many slots is
/// stale even if `LastUpdate.stale` wasn't explicitly set — mirrors
/// `solend_sdk::state::last_update::STALE_AFTER_SLOTS_ELAPSED` (currently
/// `1`), restated here rather than imported since it's a `pub const` on a
/// module this adapter doesn't otherwise need direct access to.
const SAVE_STALE_AFTER_SLOTS_ELAPSED: u64 = 1;

#[derive(Debug, Clone, Copy)]
struct ObligationSnapshot {
    slot: u64,
    close_factor_max_repay: u64,
}

#[derive(Debug, Default)]
pub struct SaveAdapter {
    reserves: HashMap<CorePubkey, ReserveSnapshot>,
    known_obligations: HashMap<CorePubkey, ObligationSnapshot>,
    last_synced_slot: u64,
    current_slot: u64,
}

impl SaveAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    fn handle_reserve(&mut self, pubkey: CorePubkey, data: &[u8], slot: u64) {
        if let Ok(reserve) = Reserve::unpack_from_slice(data) {
            self.reserves.insert(
                pubkey,
                ReserveSnapshot {
                    liquidity_mint: to_core_pubkey(reserve.liquidity.mint_pubkey),
                    slot,
                    last_update_slot: reserve.last_update.slot,
                    last_update_stale_flag: reserve.last_update.stale,
                },
            );
        }
    }

    /// Oracle/account staleness for a given reserve, per
    /// `docs/RUNBOOK.md`'s production readiness checklist. Unlike Kamino's
    /// wall-clock-timestamp field (`crates/health/src/adapters/kamino.rs`),
    /// Save's staleness is slot-denominated, so this can be checked
    /// against `current_slot` alone — no wall-clock time needed. Still an
    /// inherent method rather than part of `gyrfalcon_core::HealthAdapter`
    /// for consistency with the Kamino adapter's equivalent, and because
    /// the trait's `on_account_update` return shape has no room for a
    /// third staleness output alongside a breach candidate.
    ///
    /// Returns `None` if this reserve hasn't been observed yet.
    pub fn reserve_price_is_stale(&self, reserve: CorePubkey, current_slot: u64) -> Option<bool> {
        let snapshot = self.reserves.get(&reserve)?;
        let slots_elapsed = current_slot.saturating_sub(snapshot.last_update_slot);
        Some(snapshot.last_update_stale_flag || slots_elapsed >= SAVE_STALE_AFTER_SLOTS_ELAPSED)
    }

    fn handle_obligation(
        &mut self,
        pubkey: CorePubkey,
        data: &[u8],
        slot: u64,
    ) -> Option<BreachCandidate> {
        let obligation = Obligation::unpack_from_slice(data).ok()?;

        let borrowed = decimal_to_f64(obligation.borrowed_value);
        if borrowed <= 0.0 {
            self.known_obligations.insert(pubkey, ObligationSnapshot { slot, close_factor_max_repay: 0 });
            return None;
        }
        let unhealthy = decimal_to_f64(obligation.unhealthy_borrow_value);
        let health_factor = unhealthy / borrowed;
        if health_factor >= 1.0 {
            self.known_obligations.insert(pubkey, ObligationSnapshot { slot, close_factor_max_repay: 0 });
            return None;
        }

        let liquidity = obligation
            .borrows
            .iter()
            .max_by(|a, b| a.market_value.cmp(&b.market_value))?;
        let debt_reserve_key = to_core_pubkey(liquidity.borrow_reserve);
        let debt_reserve = self.reserves.get(&debt_reserve_key)?;

        let collateral = obligation
            .deposits
            .iter()
            .max_by(|a, b| a.market_value.cmp(&b.market_value))?;
        let collateral_reserve_key = to_core_pubkey(collateral.deposit_reserve);
        let collateral_reserve = self.reserves.get(&collateral_reserve_key)?;

        // Full outstanding borrow, not a 20% slice — see the module doc's
        // "vestigial constants" section.
        let close_factor_max_repay = decimal_to_f64(liquidity.borrowed_amount_wads).max(0.0) as u64;

        self.known_obligations.insert(
            pubkey,
            ObligationSnapshot {
                slot,
                close_factor_max_repay,
            },
        );

        Some(BreachCandidate {
            protocol: Protocol::Save,
            position_id: pubkey,
            collateral_mint: collateral_reserve.liquidity_mint,
            debt_mint: debt_reserve.liquidity_mint,
            health_factor,
            close_factor_max_repay,
            slot,
        })
    }
}

impl HealthAdapter for SaveAdapter {
    fn on_account_update(&mut self, update: AccountUpdate) -> Option<BreachCandidate> {
        if update.owner != save_program_id() {
            return None;
        }
        self.current_slot = self.current_slot.max(update.slot);

        // Save accounts have no zero-copy discriminator prefix (see the
        // module doc) — instead, disambiguate by attempting each unpack
        // in turn (LendingMarket first, since it's a fixed small size and
        // cheapest to rule out). A byte buffer that fails all three is
        // simply not one of the account types this adapter tracks.
        if let Ok(market) = LendingMarket::unpack_from_slice(&update.data) {
            let _ = market; // no market-level config needed post-vestigial-constants finding
            self.last_synced_slot = update.slot;
            return None;
        }
        if Reserve::unpack_from_slice(&update.data).is_ok() {
            self.handle_reserve(update.pubkey, &update.data, update.slot);
            self.last_synced_slot = update.slot;
            return None;
        }
        if Obligation::unpack_from_slice(&update.data).is_ok() {
            self.last_synced_slot = update.slot;
            return self.handle_obligation(update.pubkey, &update.data, update.slot);
        }
        None
    }

    fn close_factor(&self, position_id: CorePubkey) -> u64 {
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
    use solend_sdk::math::{Decimal, TryMul};
    use solend_sdk::state::{
        LastUpdate, ObligationCollateral, ObligationLiquidity, ReserveLiquidity,
    };

    fn program_owner() -> CorePubkey {
        save_program_id()
    }

    fn pk(byte: u8) -> solana_program::pubkey::Pubkey {
        solana_program::pubkey::Pubkey::new_from_array([byte; 32])
    }

    fn core_pk(byte: u8) -> CorePubkey {
        CorePubkey::new([byte; 32])
    }

    fn pack_reserve(mint: solana_program::pubkey::Pubkey) -> Vec<u8> {
        let mut reserve = Reserve::default();
        reserve.version = 1;
        reserve.last_update = LastUpdate::new(1);
        reserve.liquidity = ReserveLiquidity {
            mint_pubkey: mint,
            ..Default::default()
        };
        let mut buf = vec![0u8; Reserve::LEN];
        Reserve::pack(reserve, &mut buf).unwrap();
        buf
    }

    /// Same as [`pack_reserve`] but with `last_update.stale` explicitly
    /// cleared via `update_slot`, as Save's own program does after a
    /// refresh instruction — `LastUpdate::new` alone always starts
    /// `stale: true` by Save's own design (see `save.rs`'s staleness
    /// section), so this variant exists to exercise the "genuinely fresh"
    /// case.
    fn pack_reserve_freshly_updated(mint: solana_program::pubkey::Pubkey, slot: u64) -> Vec<u8> {
        let mut reserve = Reserve::default();
        reserve.version = 1;
        reserve.last_update = LastUpdate::new(slot);
        reserve.last_update.update_slot(slot);
        reserve.liquidity = ReserveLiquidity {
            mint_pubkey: mint,
            ..Default::default()
        };
        let mut buf = vec![0u8; Reserve::LEN];
        Reserve::pack(reserve, &mut buf).unwrap();
        buf
    }

    fn pack_obligation(
        debt_reserve: solana_program::pubkey::Pubkey,
        collateral_reserve: solana_program::pubkey::Pubkey,
        borrowed_usd: u64,
        unhealthy_usd: u64,
        borrowed_native: u64,
    ) -> Vec<u8> {
        let mut obligation = Obligation::default();
        obligation.version = 1;
        obligation.last_update = LastUpdate::new(1);
        obligation.borrowed_value = Decimal::from(borrowed_usd);
        obligation.unhealthy_borrow_value = Decimal::from(unhealthy_usd);

        let mut liquidity = ObligationLiquidity::new(debt_reserve, Decimal::one());
        liquidity.market_value = Decimal::from(borrowed_usd);
        liquidity.borrowed_amount_wads = Decimal::from(borrowed_native);
        obligation.borrows = vec![liquidity];

        let mut collateral = ObligationCollateral::new(collateral_reserve);
        collateral.market_value = Decimal::from(borrowed_usd)
            .try_mul(Decimal::from(2u64))
            .unwrap();
        collateral.deposited_amount = 1_000_000;
        obligation.deposits = vec![collateral];

        let mut buf = vec![0u8; Obligation::LEN];
        Obligation::pack(obligation, &mut buf).unwrap();
        buf
    }

    #[test]
    fn breached_obligation_produces_candidate_with_full_borrow_as_max_repay() {
        let mut adapter = SaveAdapter::new();
        let debt_mint = pk(9);
        let collateral_mint = pk(8);

        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: pack_reserve(debt_mint),
            slot: 100,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(2),
            owner: program_owner(),
            data: pack_reserve(collateral_mint),
            slot: 100,
        });

        let obligation_data = pack_obligation(pk(1), pk(2), 1_200, 1_000, 1_200_000_000);
        let candidate = adapter
            .on_account_update(AccountUpdate {
                pubkey: core_pk(42),
                owner: program_owner(),
                data: obligation_data,
                slot: 101,
            })
            .expect("undercollateralized obligation should breach");

        assert_eq!(candidate.protocol, Protocol::Save);
        assert!(candidate.health_factor < 1.0);
        assert_eq!(candidate.debt_mint, to_core_pubkey(debt_mint));
        assert_eq!(candidate.collateral_mint, to_core_pubkey(collateral_mint));
        // Full borrow, not 20% of it — see module doc.
        assert!(candidate.close_factor_max_repay >= 1_199_000_000);
    }

    #[test]
    fn healthy_obligation_produces_no_candidate() {
        let mut adapter = SaveAdapter::new();
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: pack_reserve(pk(9)),
            slot: 100,
        });
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(2),
            owner: program_owner(),
            data: pack_reserve(pk(8)),
            slot: 100,
        });

        let obligation_data = pack_obligation(pk(1), pk(2), 500, 1_000, 500_000_000);
        let result = adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(42),
            owner: program_owner(),
            data: obligation_data,
            slot: 101,
        });
        assert!(result.is_none());
    }

    #[test]
    fn non_save_owner_is_ignored() {
        let mut adapter = SaveAdapter::new();
        let result = adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: core_pk(255),
            data: vec![0u8; 64],
            slot: 1,
        });
        assert!(result.is_none());
        assert_eq!(adapter.position_count(), 0);
    }

    #[test]
    fn reserve_freshly_updated_this_slot_is_not_stale() {
        let mut adapter = SaveAdapter::new();
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: pack_reserve_freshly_updated(pk(9), 500),
            slot: 500,
        });
        assert_eq!(adapter.reserve_price_is_stale(core_pk(1), 500), Some(false));
    }

    #[test]
    fn reserve_untouched_for_more_than_one_slot_is_stale() {
        let mut adapter = SaveAdapter::new();
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: pack_reserve_freshly_updated(pk(9), 500),
            slot: 500,
        });
        // Save's STALE_AFTER_SLOTS_ELAPSED is 1 — even one slot later, stale.
        assert_eq!(adapter.reserve_price_is_stale(core_pk(1), 502), Some(true));
    }

    #[test]
    fn reserve_never_explicitly_refreshed_is_stale_by_default() {
        // pack_reserve (not the _freshly_updated variant) uses
        // LastUpdate::new alone, which starts stale=true by Save's own
        // design until an explicit refresh clears it.
        let mut adapter = SaveAdapter::new();
        adapter.on_account_update(AccountUpdate {
            pubkey: core_pk(1),
            owner: program_owner(),
            data: pack_reserve(pk(9)),
            slot: 1,
        });
        assert_eq!(adapter.reserve_price_is_stale(core_pk(1), 1), Some(true));
    }

    #[test]
    fn reserve_staleness_is_unknown_for_an_unobserved_reserve() {
        let adapter = SaveAdapter::new();
        assert_eq!(adapter.reserve_price_is_stale(core_pk(99), 1), None);
    }
}
