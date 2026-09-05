//! Mint-keyed flash-source router — `gyrfalcon_core::FlashSourceRouter`,
//! ranking candidate reserves by depth and fee per Whitepaper Eq. 3.
//!
//! # Primary & Fallback Flash Liquidity Sources
//!
//! Per `01_PROTOCOLS.md §3` and `00_MIGRATION_AUDIT.md §1`:
//! - **Primary**: Kamino Lend Flash Borrow (0.00% fee, 12,000 CU)
//! - **Fallback**: Solend (Save) Flash Loans (0.00% fee, 15,000 CU)
//!
//! MarginFi has no flash-loan role and is excluded.

pub mod dex;
pub use dex::{DexRouteDecision, DexRouter, DexVenue, MarketQuote};

use gyrfalcon_core::types::FlashSource;
use gyrfalcon_core::{FlashSourceRouter, Protocol, Pubkey as CorePubkey};
use klend_interface::state::{
    from_account_data as kamino_from_account_data, Reserve as KaminoReserve, SplDiscriminate,
};
use solana_program::program_pack::Pack;
use solend_sdk::state::Reserve as SaveReserve;
use std::collections::HashMap;

fn kamino_pubkey_to_core(p: solana_pubkey::Pubkey) -> CorePubkey {
    CorePubkey::new(p.to_bytes())
}

fn save_pubkey_to_core(p: solana_program::pubkey::Pubkey) -> CorePubkey {
    CorePubkey::new(p.to_bytes())
}

/// `u64::MAX` marks a Kamino reserve's flash loan as disabled, per
/// `klend_interface::state::ReserveFees::flash_loan_fee_sf`'s doc comment.
const KAMINO_FLASH_LOAN_DISABLED: u64 = u64::MAX;

fn kamino_fee_bps_from_sf(flash_loan_fee_sf: u64) -> Option<u32> {
    if flash_loan_fee_sf == KAMINO_FLASH_LOAN_DISABLED {
        return None;
    }
    let rate = klend_interface::Fraction::from_bits(flash_loan_fee_sf as u128).to_num::<f64>();
    Some((rate * 10_000.0).round() as u32)
}

/// Save's `flash_loan_fee_wad` is a Wad (`10^18` = 100%), with `0` simply
/// meaning free — no disabled sentinel, unlike Kamino's `u64::MAX`.
fn save_fee_bps_from_wad(flash_loan_fee_wad: u64) -> u32 {
    let rate = flash_loan_fee_wad as f64 / 1e18;
    (rate * 10_000.0).round() as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashProvider {
    Kamino,
    Solend,
}

#[derive(Debug, Clone, Copy)]
struct ReserveInfo {
    provider: FlashProvider,
    mint: CorePubkey,
    available_liquidity: u64,
    /// `None` when this reserve's flash loans are disabled (Kamino only —
    /// Save has no disabled sentinel, see `save_fee_bps_from_wad`).
    fee_bps: Option<u32>,
}

/// `gyrfalcon_core::FlashSourceRouter` backed by Kamino (primary) and Solend
/// (fallback) reserves per `01_PROTOCOLS.md §3`.
#[derive(Debug, Clone, Default)]
pub struct MultiSourceRouter {
    reserves: HashMap<CorePubkey, ReserveInfo>,
    save_live_verified: bool,
}

impl MultiSourceRouter {
    pub fn new() -> Self {
        Self {
            reserves: HashMap::new(),
            save_live_verified: false,
        }
    }

    /// Register a reserve directly (used in replay, simulation, and tests).
    pub fn add_reserve(
        &mut self,
        reserve: CorePubkey,
        provider: FlashProvider,
        mint: CorePubkey,
        available_liquidity: u64,
        fee_bps: Option<u32>,
    ) {
        self.reserves.insert(
            reserve,
            ReserveInfo {
                provider,
                mint,
                available_liquidity,
                fee_bps,
            },
        );
    }

    /// Enable or disable live flash-loan selection for Save reserves.
    ///
    /// Per `01_PROTOCOLS.md §3`: Solend (Save) is the fallback flash-loan
    /// provider behind Kamino's native flash borrow. Live routing falls
    /// through to Kamino until Save is verified on-chain.
    pub fn set_save_live_verified(&mut self, verified: bool) {
        self.save_live_verified = verified;
    }

    /// Query whether Save flash loans are enabled for live execution.
    pub fn is_save_live_verified(&self) -> bool {
        self.save_live_verified
    }

    /// Rank all observed candidate sources without gating, for offline telemetry,
    /// simulation, and depth/fee comparison across sources (Whitepaper Eq. 3).
    pub fn rank_all_sources(&self, mint: CorePubkey, amount: u64) -> Vec<FlashSource> {
        let mut candidates: Vec<FlashSource> = self
            .reserves
            .iter()
            .filter(|(_, info)| info.mint == mint)
            .filter(|(_, info)| info.available_liquidity >= amount)
            .filter_map(|(reserve_key, info)| {
                info.fee_bps.map(|fee_bps| FlashSource {
                    protocol: Protocol::Kamino,
                    reserve: *reserve_key,
                    fee_bps,
                })
            })
            .collect();

        candidates.sort_by(|a, b| {
            let a_info = self.reserves.get(&a.reserve);
            let b_info = self.reserves.get(&b.reserve);
            let a_fee = a.fee_bps;
            let b_fee = b.fee_bps;
            let a_liq = a_info.map(|r| r.available_liquidity).unwrap_or(0);
            let b_liq = b_info.map(|r| r.available_liquidity).unwrap_or(0);
            a_fee.cmp(&b_fee).then(b_liq.cmp(&a_liq))
        });

        candidates
    }

    /// Feed one raw Kamino-program account update. No-op for anything
    /// that isn't a `Reserve` account.
    pub fn observe_kamino_account(&mut self, pubkey: CorePubkey, data: &[u8]) {
        if data.len() < 8 || &data[..8] != KaminoReserve::SPL_DISCRIMINATOR_SLICE {
            return;
        }
        let Ok(reserve) = kamino_from_account_data::<KaminoReserve>(data) else {
            return;
        };
        self.reserves.insert(
            pubkey,
            ReserveInfo {
                provider: FlashProvider::Kamino,
                mint: kamino_pubkey_to_core(reserve.liquidity.mint_pubkey),
                available_liquidity: reserve.available_liquidity(),
                fee_bps: kamino_fee_bps_from_sf(reserve.config.fees.flash_loan_fee_sf),
            },
        );
    }

    /// Feed one raw Save-program account update. No-op for anything that
    /// doesn't unpack as a `Reserve` (see
    /// `00_MIGRATION_AUDIT.md` §1 for Solend flash-loan fallback role).
    pub fn observe_save_account(&mut self, pubkey: CorePubkey, data: &[u8]) {
        let Ok(reserve) = SaveReserve::unpack_from_slice(data) else {
            return;
        };
        self.reserves.insert(
            pubkey,
            ReserveInfo {
                provider: FlashProvider::Solend,
                mint: save_pubkey_to_core(reserve.liquidity.mint_pubkey),
                available_liquidity: reserve.liquidity.available_amount,
                fee_bps: Some(save_fee_bps_from_wad(
                    reserve.config.fees.flash_loan_fee_wad,
                )),
            },
        );
    }

    pub fn tracked_reserve_count(&self) -> usize {
        self.reserves.len()
    }
}

impl FlashSourceRouter for MultiSourceRouter {
    fn route(&self, mint: CorePubkey, amount: u64) -> Option<FlashSource> {
        self.reserves
            .iter()
            .filter(|(_, info)| info.mint == mint)
            .filter(|(_, info)| info.available_liquidity >= amount)
            .filter(|(_, info)| info.provider != FlashProvider::Solend || self.save_live_verified)
            .filter_map(|(reserve_key, info)| {
                info.fee_bps.map(|fee_bps| (reserve_key, info, fee_bps))
            })
            // Lowest fee wins; if tied, Kamino (primary) preferred over Solend (fallback); then deepest liquidity
            .min_by(|(_, a, a_fee), (_, b, b_fee)| {
                a_fee
                    .cmp(b_fee)
                    .then_with(|| match (a.provider, b.provider) {
                        (FlashProvider::Kamino, FlashProvider::Solend) => std::cmp::Ordering::Less,
                        (FlashProvider::Solend, FlashProvider::Kamino) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    })
                    .then(b.available_liquidity.cmp(&a.available_liquidity))
            })
            .map(|(reserve_key, _, fee_bps)| FlashSource {
                protocol: Protocol::Kamino,
                reserve: *reserve_key,
                fee_bps,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    fn encode_kamino(reserve: &KaminoReserve) -> Vec<u8> {
        let mut out = KaminoReserve::SPL_DISCRIMINATOR_SLICE.to_vec();
        out.extend_from_slice(bytemuck::bytes_of(reserve));
        out
    }

    fn sample_kamino_reserve(
        mint: solana_pubkey::Pubkey,
        available: u64,
        fee_bps: Option<u32>,
    ) -> KaminoReserve {
        let mut reserve = KaminoReserve::zeroed();
        reserve.liquidity.mint_pubkey = mint;
        reserve.liquidity.total_available_amount = available;
        reserve.config.fees.flash_loan_fee_sf = match fee_bps {
            None => KAMINO_FLASH_LOAN_DISABLED,
            Some(bps) => ((bps as f64 / 10_000.0) * (1u128 << 60) as f64) as u64,
        };
        reserve
    }

    fn encode_save(reserve: SaveReserve) -> Vec<u8> {
        let mut buf = vec![0u8; SaveReserve::LEN];
        SaveReserve::pack(reserve, &mut buf).unwrap();
        buf
    }

    fn sample_save_reserve(
        mint: solana_program::pubkey::Pubkey,
        available: u64,
        fee_bps: u32,
    ) -> SaveReserve {
        let mut reserve = SaveReserve::default();
        reserve.liquidity.mint_pubkey = mint;
        reserve.liquidity.available_amount = available;
        reserve.config.fees.flash_loan_fee_wad = (fee_bps as f64 / 10_000.0 * 1e18) as u64;
        reserve
    }

    fn kamino_pk(byte: u8) -> solana_pubkey::Pubkey {
        solana_pubkey::Pubkey::new_from_array([byte; 32])
    }

    fn save_pk(byte: u8) -> solana_program::pubkey::Pubkey {
        solana_program::pubkey::Pubkey::new_from_array([byte; 32])
    }

    fn core_pk(byte: u8) -> CorePubkey {
        CorePubkey::new([byte; 32])
    }

    #[test]
    fn routes_across_both_protocols_picking_lowest_fee() {
        let mut router = MultiSourceRouter::new();
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(9), 1_000_000, Some(10))),
        );
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(9), 1_000_000, 3)),
        );

        // Before live verification: Save offers a 3 bps fee vs Kamino's 10 bps,
        // but Save is unverified for live execution (01_PROTOCOLS.md §3).
        // Live routing MUST fall through to Kamino!
        let unverified_live = router.route(core_pk(9), 500_000).expect("should fall through to Kamino");
        assert_eq!(unverified_live.protocol, Protocol::Kamino);
        assert_eq!(unverified_live.fee_bps, 10);
        assert_eq!(unverified_live.reserve, core_pk(1));

        // Unrestricted ranking (telemetry / simulation) sees Save as lower fee:
        let all_sources = router.rank_all_sources(core_pk(9), 500_000);
        assert_eq!(all_sources.len(), 2);
        assert_eq!(all_sources[0].reserve, core_pk(2));
        assert_eq!(all_sources[0].fee_bps, 3);

        // After live verification is enabled: Save is selected as the live flash source!
        router.set_save_live_verified(true);
        let verified_live = router.route(core_pk(9), 500_000).expect("should route to Save");
        assert_eq!(verified_live.protocol, Protocol::Kamino);
        assert_eq!(verified_live.fee_bps, 3);
        assert_eq!(verified_live.reserve, core_pk(2));
    }

    #[test]
    fn falls_back_to_kamino_when_save_lacks_depth() {
        let mut router = MultiSourceRouter::new();
        router.set_save_live_verified(true);
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(9), 1_000_000, Some(10))),
        );
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(9), 100, 3)),
        ); // too shallow

        let source = router.route(core_pk(9), 500_000).expect("should route");
        assert_eq!(source.protocol, Protocol::Kamino);
        assert_eq!(source.reserve, core_pk(1));
    }

    #[test]
    fn skips_kamino_reserves_with_flash_loans_disabled() {
        let mut router = MultiSourceRouter::new();
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(9), 1_000_000, None)), // disabled
        );
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(9), 1_000_000, 7)),
        );

        // When Kamino is disabled and Save is unverified: cannot route live
        assert!(router.route(core_pk(9), 500_000).is_none());

        // Once Save is verified: routes to Save
        router.set_save_live_verified(true);
        let source = router.route(core_pk(9), 500_000).unwrap();
        assert_eq!(source.protocol, Protocol::Kamino);
        assert_eq!(source.fee_bps, 7);
        assert_eq!(source.reserve, core_pk(2));
    }

    #[test]
    fn tracked_reserve_count_reflects_both_protocols() {
        let mut router = MultiSourceRouter::new();
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(9), 1, Some(1))),
        );
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(9), 1, 1)),
        );
        assert_eq!(router.tracked_reserve_count(), 2);
    }

    #[test]
    fn returns_none_for_unknown_mint() {
        let router = MultiSourceRouter::new();
        assert!(router.route(core_pk(42), 1).is_none());
    }

    #[test]
    fn test_unverified_save_is_bypassed_in_favor_of_kamino() {
        let mut router = MultiSourceRouter::new();
        assert!(!router.is_save_live_verified());

        // Save has lower fee (1 bps) and deeper liquidity (10M) than Kamino (5 bps, 2M)
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(5), 2_000_000, Some(5))),
        );
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(5), 10_000_000, 1)),
        );

        // Unverified: must fall through to Kamino
        let live_pick = router.route(core_pk(5), 1_000_000).expect("must route to Kamino");
        assert_eq!(live_pick.protocol, Protocol::Kamino);
        assert_eq!(live_pick.reserve, core_pk(1));
        assert_eq!(live_pick.fee_bps, 5);

        // Now enable Save verification: must switch to Save
        router.set_save_live_verified(true);
        assert!(router.is_save_live_verified());

        let verified_pick = router.route(core_pk(5), 1_000_000).expect("must route to Save");
        assert_eq!(verified_pick.protocol, Protocol::Kamino);
        assert_eq!(verified_pick.reserve, core_pk(2));
        assert_eq!(verified_pick.fee_bps, 1);
    }

    #[test]
    fn test_unverified_save_sole_source_returns_none() {
        let mut router = MultiSourceRouter::new();
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(7), 5_000_000, 2)),
        );

        // Sole source is unverified Save -> cannot route
        assert!(router.route(core_pk(7), 100_000).is_none());

        // Gating unlocked -> routes
        router.set_save_live_verified(true);
        let pick = router.route(core_pk(7), 100_000).expect("should route once verified");
        assert_eq!(pick.protocol, Protocol::Kamino);
        assert_eq!(pick.reserve, core_pk(2));
    }

    #[test]
    fn test_prefers_kamino_over_save_on_equal_fee() {
        let mut router = MultiSourceRouter::new();
        router.set_save_live_verified(true);

        // Both Kamino and Save offer 0 bps fee and equal depth
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(10), 1_000_000, 0)),
        );
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(10), 1_000_000, Some(0))),
        );

        // Kamino is primary flash source per 01_PROTOCOLS.md §3 -> must pick Kamino
        let pick = router.route(core_pk(10), 500_000).expect("must route");
        assert_eq!(pick.reserve, core_pk(1));
    }
}
