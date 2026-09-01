//! Position sizing — `docs/STRATEGY.md#position-sizing`.
//!
//! "The engine repays the maximum allowed by the close factor... unless
//! flash-source depth or route feasibility reduces it." Route-feasibility
//! stepping (the CU/byte-fit case) needs `bundler` (Stage B item 9) to
//! exist first to know whether a route fits — this module implements the
//! two axes that don't: close-factor ceiling and flash-source depth.

use gyrfalcon_core::BreachCandidate;

/// `r` in Whitepaper Eq. 2/3 — the amount to actually repay for one
/// candidate, after applying every sizing constraint currently
/// implementable (close factor, flash depth). Route-feasibility stepping
/// is not yet applied — see the module doc.
pub fn size_position(candidate: &BreachCandidate, flash_source_available: u64) -> u64 {
    candidate.close_factor_max_repay.min(flash_source_available)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::{Protocol, Pubkey};

    fn candidate(close_factor_max_repay: u64) -> BreachCandidate {
        BreachCandidate {
            protocol: Protocol::Kamino,
            position_id: Pubkey::new([1u8; 32]),
            collateral_mint: Pubkey::new([2u8; 32]),
            debt_mint: Pubkey::new([3u8; 32]),
            health_factor: 0.9,
            close_factor_max_repay,
            slot: 100,
        }
    }

    #[test]
    fn repays_full_close_factor_when_flash_depth_is_sufficient() {
        let c = candidate(1_000_000);
        assert_eq!(size_position(&c, 5_000_000), 1_000_000);
    }

    #[test]
    fn caps_at_flash_source_depth_when_shallower_than_close_factor() {
        let c = candidate(1_000_000);
        assert_eq!(size_position(&c, 400_000), 400_000);
    }

    #[test]
    fn never_sizes_down_below_available_depth_purely_to_reduce_risk() {
        // STRATEGY.md: "sizing does not go the other way" — with ample
        // depth, the full close-factor amount is used, not something
        // smaller.
        let c = candidate(250_000);
        assert_eq!(size_position(&c, u64::MAX), 250_000);
    }
}
