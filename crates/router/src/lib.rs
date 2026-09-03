//! Mint-keyed flash-source router — `gyrfalcon_core::FlashSourceRouter`,
//! ranking candidate reserves by depth and fee per Whitepaper Eq. 3.
//!
//! # Stage B -> Stage C
//!
//! Stage B (`docs/BUILD_ORDER.md` item 6) shipped Kamino-only. Stage C
//! item 13 adds Save as a second source — `MultiSourceRouter` tracks
//! reserves from both protocols, tagged by `Protocol`, so
//! `FlashSourceRouter::route`'s depth/fee comparison across sources
//! (Whitepaper Eq. 3) is now meaningfully exercised for the first time
//! rather than trivially picking the only option.
//!
//! **Save flash-loan caveat**: see `crates/health/src/adapters/save.rs`'s
//! module doc — Save's own docs describe their flash-loan implementation
//! as functionally limited as of this writing. This router still ranks
//! Save reserves as candidates per BUILD_ORDER item 13's instruction, but
//! that's a live-verify item before real capital depends on a Save-sourced
//! flash loan landing correctly, same spirit as MarginFi's flash-instruction
//! finding.
//!
//! Both `klend-interface` (Kamino) and `solend-sdk` (Save) apply here —
//! see `crates/health/src/adapters/kamino.rs` and `save.rs` for licensing
//! and decode-mechanism notes for each. Same MSRV 1.81 caveat as those
//! adapters (via `klend-interface`): this crate could not be built/tested
//! in the sandbox that wrote it.

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
/// meaning free — no disabled sentinel, unlike Kamino's `u64::MAX`. See
/// `crates/health/src/adapters/save.rs` for the same Wad-to-f64 pattern
/// applied to other Save `Decimal` fields.
fn save_fee_bps_from_wad(flash_loan_fee_wad: u64) -> u32 {
    let rate = flash_loan_fee_wad as f64 / 1e18;
    (rate * 10_000.0).round() as u32
}

#[derive(Debug, Clone, Copy)]
struct ReserveInfo {
    protocol: Protocol,
    mint: CorePubkey,
    available_liquidity: u64,
    /// `None` when this reserve's flash loans are disabled (Kamino only —
    /// Save has no disabled sentinel, see `save_fee_bps_from_wad`).
    fee_bps: Option<u32>,
}

/// `gyrfalcon_core::FlashSourceRouter` backed by both Kamino and Save
/// reserves, with protocol live-verification gating per `01_PROTOCOLS.md §2`.
#[derive(Debug, Clone, Default)]
pub struct MultiSourceRouter {
    reserves: HashMap<CorePubkey, ReserveInfo>,
    save_live_verified: bool,
    marginfi_live_verified: bool,
}

impl MultiSourceRouter {
    pub fn new() -> Self {
        Self {
            reserves: HashMap::new(),
            save_live_verified: false,
            marginfi_live_verified: false,
        }
    }

    /// Enable or disable live flash-loan selection for Save reserves.
    ///
    /// Per `01_PROTOCOLS.md §2 (Save)`: Save reserves may be observed and
    /// ranked for depth/fee comparison, but live routing must fall through to
    /// Kamino until verified on-chain.
    pub fn set_save_live_verified(&mut self, verified: bool) {
        self.save_live_verified = verified;
    }

    /// Query whether Save flash loans are enabled for live execution.
    pub fn is_save_live_verified(&self) -> bool {
        self.save_live_verified
    }

    /// Enable or disable live flash-loan selection for MarginFi reserves.
    pub fn set_marginfi_live_verified(&mut self, verified: bool) {
        self.marginfi_live_verified = verified;
    }

    /// Query whether MarginFi flash loans are enabled for live execution.
    pub fn is_marginfi_live_verified(&self) -> bool {
        self.marginfi_live_verified
    }

    /// Rank all observed candidate sources without gating, for offline telemetry,
    /// simulation, and depth/fee comparison across protocols (Whitepaper Eq. 3).
    pub fn rank_all_sources(&self, mint: CorePubkey, amount: u64) -> Vec<FlashSource> {
        let mut candidates: Vec<FlashSource> = self
            .reserves
            .iter()
            .filter(|(_, info)| info.mint == mint)
            .filter(|(_, info)| info.available_liquidity >= amount)
            .filter_map(|(reserve_key, info)| {
                info.fee_bps.map(|fee_bps| FlashSource {
                    protocol: info.protocol,
                    reserve: *reserve_key,
                    fee_bps,
                })
            })
            .collect();

        candidates.sort_by(|a, b| {
            let a_liq = self.reserves.get(&a.reserve).map(|r| r.available_liquidity).unwrap_or(0);
            let b_liq = self.reserves.get(&b.reserve).map(|r| r.available_liquidity).unwrap_or(0);
            a.fee_bps.cmp(&b.fee_bps).then(b_liq.cmp(&a_liq))
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
                protocol: Protocol::Kamino,
                mint: kamino_pubkey_to_core(reserve.liquidity.mint_pubkey),
                available_liquidity: reserve.available_liquidity(),
                fee_bps: kamino_fee_bps_from_sf(reserve.config.fees.flash_loan_fee_sf),
            },
        );
    }

    /// Feed one raw Save-program account update. No-op for anything that
    /// doesn't unpack as a `Reserve` (see
    /// `crates/health/src/adapters/save.rs` for why Save accounts are
    /// disambiguated by unpack-attempt rather than a discriminator byte).
    pub fn observe_save_account(&mut self, pubkey: CorePubkey, data: &[u8]) {
        let Ok(reserve) = SaveReserve::unpack_from_slice(data) else {
            return;
        };
        self.reserves.insert(
            pubkey,
            ReserveInfo {
                protocol: Protocol::Save,
                mint: save_pubkey_to_core(reserve.liquidity.mint_pubkey),
                available_liquidity: reserve.liquidity.available_amount,
                fee_bps: Some(save_fee_bps_from_wad(
                    reserve.config.fees.flash_loan_fee_wad,
                )),
            },
        );
    }

    /// Feed one raw MarginFi bank account update. No-op for anything that
    /// doesn't match MarginFi's Bank discriminator.
    pub fn observe_marginfi_account(&mut self, pubkey: CorePubkey, data: &[u8]) {
        // Anchor discriminator: sha256("account:Bank")[..8]
        const MARGINFI_BANK_DISCRIMINATOR: [u8; 8] = [142, 49, 166, 242, 50, 66, 97, 188];
        if data.len() < 72 || data[..8] != MARGINFI_BANK_DISCRIMINATOR {
            return;
        }

        // MarginFi Bank layout:
        // offset 8..40: group: Pubkey
        // offset 40..72: mint: Pubkey
        let mut mint_bytes = [0u8; 32];
        mint_bytes.copy_from_slice(&data[40..72]);
        let mint = CorePubkey::new(mint_bytes);

        // Read available liquidity if provided at offset 72..80
        let available_liquidity = if data.len() >= 80 {
            u64::from_le_bytes(data[72..80].try_into().unwrap_or([0; 8]))
        } else {
            0
        };

        self.reserves.insert(
            pubkey,
            ReserveInfo {
                protocol: Protocol::MarginFi,
                mint,
                available_liquidity,
                fee_bps: Some(0), // MarginFi intra-transaction flash loans charge 0 protocol fee
            },
        );
    }

    /// Directly record or update a MarginFi bank's observed parameters.
    pub fn observe_marginfi_bank(
        &mut self,
        pubkey: CorePubkey,
        mint: CorePubkey,
        available_liquidity: u64,
        fee_bps: Option<u32>,
    ) {
        self.reserves.insert(
            pubkey,
            ReserveInfo {
                protocol: Protocol::MarginFi,
                mint,
                available_liquidity,
                fee_bps: fee_bps.or(Some(0)),
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
            .filter(|(_, info)| match info.protocol {
                Protocol::Kamino => true,
                Protocol::Save => self.save_live_verified,
                Protocol::MarginFi => self.marginfi_live_verified,
            })
            .filter_map(|(reserve_key, info)| {
                info.fee_bps.map(|fee_bps| (reserve_key, info, fee_bps))
            })
            // Lowest fee wins, across verified protocols; ties broken by deepest
            // liquidity so the pick is deterministic given identical fees
            // — this is Whitepaper Eq. 3's comparison, now meaningfully
            // multi-source and gated on verified flash execution (01_PROTOCOLS.md §2).
            .min_by(|(_, a, a_fee), (_, b, b_fee)| {
                a_fee
                    .cmp(b_fee)
                    .then(b.available_liquidity.cmp(&a.available_liquidity))
            })
            .map(|(reserve_key, info, fee_bps)| FlashSource {
                protocol: info.protocol,
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
        // Same logical mint, bytes [9;32], seen through each protocol's
        // own Pubkey type independently -- this is exactly the "two
        // options to compare" case Stage C item 13 exists to exercise.
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(9), 1_000_000, Some(10))),
        );
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(9), 1_000_000, 3)),
        );

        // Before live verification: Save offers a 3 bps fee vs Kamino's 10 bps,
        // but Save is unverified for live execution (01_PROTOCOLS.md §2).
        // Live routing MUST fall through to Kamino!
        let unverified_live = router.route(core_pk(9), 500_000).expect("should fall through to Kamino");
        assert_eq!(unverified_live.protocol, Protocol::Kamino);
        assert_eq!(unverified_live.fee_bps, 10);
        assert_eq!(unverified_live.reserve, core_pk(1));

        // Unrestricted ranking (telemetry / simulation) sees Save as lower fee:
        let all_sources = router.rank_all_sources(core_pk(9), 500_000);
        assert_eq!(all_sources.len(), 2);
        assert_eq!(all_sources[0].protocol, Protocol::Save);
        assert_eq!(all_sources[0].fee_bps, 3);

        // After live verification is enabled: Save is selected as the live flash source!
        router.set_save_live_verified(true);
        let verified_live = router.route(core_pk(9), 500_000).expect("should route to Save");
        assert_eq!(verified_live.protocol, Protocol::Save);
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
        assert_eq!(source.protocol, Protocol::Save);
        assert_eq!(source.fee_bps, 7);
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
        assert_eq!(verified_pick.protocol, Protocol::Save);
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
        assert_eq!(pick.protocol, Protocol::Save);
    }

    #[test]
    fn test_unverified_marginfi_is_bypassed_in_favor_of_kamino() {
        let mut router = MultiSourceRouter::new();
        assert!(!router.is_marginfi_live_verified());

        // MarginFi has 0 bps fee, Kamino has 8 bps fee
        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(11), 1_000_000, Some(8))),
        );
        router.observe_marginfi_bank(core_pk(2), core_pk(11), 2_000_000, Some(0));

        // When unverified: MarginFi is excluded, falls through to Kamino
        let unverified_live = router.route(core_pk(11), 500_000).expect("should route to Kamino");
        assert_eq!(unverified_live.protocol, Protocol::Kamino);
        assert_eq!(unverified_live.reserve, core_pk(1));
        assert_eq!(unverified_live.fee_bps, 8);

        // When verified: MarginFi is selected (0 bps vs 8 bps)
        router.set_marginfi_live_verified(true);
        assert!(router.is_marginfi_live_verified());

        let verified_live = router.route(core_pk(11), 500_000).expect("should route to MarginFi");
        assert_eq!(verified_live.protocol, Protocol::MarginFi);
        assert_eq!(verified_live.reserve, core_pk(2));
        assert_eq!(verified_live.fee_bps, 0);
    }

    #[test]
    fn test_marginfi_raw_bank_account_observation() {
        let mut router = MultiSourceRouter::new();

        // Construct a raw buffer with Anchor Bank discriminator + group + mint + liquidity
        const MARGINFI_BANK_DISCRIMINATOR: [u8; 8] = [142, 49, 166, 242, 50, 66, 97, 188];
        let mut raw_data = Vec::new();
        raw_data.extend_from_slice(&MARGINFI_BANK_DISCRIMINATOR);
        raw_data.extend_from_slice(&[10u8; 32]); // group (offset 8..40)
        raw_data.extend_from_slice(&[13u8; 32]); // mint (offset 40..72)
        raw_data.extend_from_slice(&5_000_000u64.to_le_bytes()); // available liquidity (offset 72..80)

        router.observe_marginfi_account(core_pk(3), &raw_data);
        assert_eq!(router.tracked_reserve_count(), 1);

        // Unverified -> None
        assert!(router.route(core_pk(13), 100_000).is_none());

        // Verified -> Routes to MarginFi
        router.set_marginfi_live_verified(true);
        let pick = router.route(core_pk(13), 100_000).expect("should route to MarginFi");
        assert_eq!(pick.protocol, Protocol::MarginFi);
        assert_eq!(pick.fee_bps, 0);
        assert_eq!(pick.reserve, core_pk(3));
    }

    #[test]
    fn test_rank_all_sources_places_marginfi_first_when_zero_fee() {
        let mut router = MultiSourceRouter::new();

        router.observe_kamino_account(
            core_pk(1),
            &encode_kamino(&sample_kamino_reserve(kamino_pk(22), 1_000_000, Some(10))),
        );
        router.observe_save_account(
            core_pk(2),
            &encode_save(sample_save_reserve(save_pk(22), 1_000_000, 3)),
        );
        router.observe_marginfi_bank(core_pk(3), core_pk(22), 1_000_000, Some(0));

        let ranked = router.rank_all_sources(core_pk(22), 500_000);
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].protocol, Protocol::MarginFi);
        assert_eq!(ranked[0].fee_bps, 0);
        assert_eq!(ranked[1].protocol, Protocol::Save);
        assert_eq!(ranked[1].fee_bps, 3);
        assert_eq!(ranked[2].protocol, Protocol::Kamino);
        assert_eq!(ranked[2].fee_bps, 10);
    }
}
