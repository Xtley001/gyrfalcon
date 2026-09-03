//! Tip bidding — Stage B item 8's "static tip floor" slice of
//! `docs/STRATEGY.md#tip-bidding-model`.
//!
//! The full model is `tip = clamp(floor_tip + k * contention(reserve) * (r
//! * b), min = risk.min_tip_usd, max = min(max_tip_pct_of_bonus * r * b,
//! max_tip_per_tx_usd))`. The contention-scaled term (`k *
//! contention(reserve)`) is explicitly Stage C (`docs/BUILD_ORDER.md` item
//! 15) — `k` "is recalibrated from landing-rate data... not hand-tuned
//! once and left static," which needs `observe`-mode data this build has
//! no way to collect. This module implements the static floor only:
//! `tip = clamp(floor_tip, min = 0, max = min(max_tip_pct_of_bonus * r *
//! b, max_tip_per_tx_usd))`.
//!
//! # A doc inconsistency, noted rather than silently resolved
//!
//! `docs/STRATEGY.md`'s formula references `risk.min_tip_usd`, but
//! `docs/CONFIGURATION.md`'s `[risk]` table (and
//! `config/gyrfalcon.example.toml`) has no such field — only
//! `min_profit_usd`, `max_tip_per_tx_usd`, `max_tip_per_slot_usd`, and
//! `max_tip_pct_of_bonus`. Rather than inventing a config field that isn't
//! in the schema two other docs agree on, [`STATIC_TIP_FLOOR_USD`] is a
//! module-level placeholder constant. Flag this doc mismatch and add the
//! field to `docs/CONFIGURATION.md` + the example configs together, in the
//! same change, once Stage C's calibration work (item 15) determines
//! whether a nonzero floor is actually needed.

use gyrfalcon_config::RiskConfig;

/// Placeholder static tip floor — see the module doc's "doc inconsistency"
/// section for why this isn't sourced from `RiskConfig`.
pub const STATIC_TIP_FLOOR_USD: f64 = 0.0;

/// Computes the tip bid for one candidate under Stage B's static-floor
/// model. `bonus_usd` is `b` and `repay_usd` folds into it already per
/// `ProfitEstimate::bonus_usd` (Whitepaper Eq. 2) — this function does not
/// re-derive `r * b`, it takes the bonus value directly.
pub fn static_tip_bid(bonus_usd: f64, risk: &RiskConfig) -> f64 {
    let ceiling = (risk.max_tip_pct_of_bonus * bonus_usd).min(risk.max_tip_per_tx_usd);
    let floor = risk.min_tip_usd.max(STATIC_TIP_FLOOR_USD);
    let competitive = risk.max_tip_pct_of_bonus * bonus_usd;
    competitive.max(floor).clamp(0.0, ceiling.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn risk(max_tip_pct_of_bonus: f64, max_tip_per_tx_usd: f64) -> RiskConfig {
        RiskConfig {
            min_profit_usd: 5.0,
            min_tip_usd: 0.0,
            sync_lag_halt_slots: 3,
            contention_ceiling: 0.8,
            max_tip_per_tx_usd,
            max_tip_per_slot_usd: 600.0,
            max_tip_pct_of_bonus,
            max_drawdown_usd: 1000.0,
            consecutive_revert_limit: 3,
        }
    }

    #[test]
    fn tip_never_exceeds_pct_of_bonus_ceiling() {
        let r = risk(0.4, 1000.0);
        let tip = static_tip_bid(10.0, &r);
        assert!(tip <= 0.4 * 10.0);
    }

    #[test]
    fn tip_never_exceeds_absolute_per_tx_ceiling_even_on_a_huge_bonus() {
        let r = risk(0.4, 50.0);
        let tip = static_tip_bid(10_000.0, &r); // 40% of this would be $4000
        assert!(tip <= 50.0);
    }

    #[test]
    fn tip_is_never_negative() {
        let r = risk(0.4, 150.0);
        let tip = static_tip_bid(0.0, &r);
        assert!(tip >= 0.0);
    }
}
