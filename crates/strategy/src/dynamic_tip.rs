//! Dynamic tip-bidding curve — Whitepaper Eq. 2b, `docs/STRATEGY.md#tip-bidding-model`
//! and `docs/STRATEGY.md#cold-start`.
//!
//! ```text
//! tip = clamp(
//!   floor_tip + k * contention(reserve) * (r * b),
//!   min = risk.min_tip_usd,
//!   max = min(risk.max_tip_pct_of_bonus * r * b, risk.max_tip_per_tx_usd)
//! )
//! ```
//!
//! # This module does not ship a calibrated `k`
//!
//! `docs/BUILD_ORDER.md`'s "What not to do": *"Do not implement the
//! dynamic tip curve (Eq. 2b) before there is `observe`-mode data to
//! calibrate it against — a curve fit to nothing is a guess wearing a
//! formula."* This build has run no `observe`-mode session against
//! mainnet (no live Geyser/RPC access — see Stage A/B's notes throughout
//! this codebase), so there is no landing-rate data to calibrate `k` or
//! the per-reserve contention map from.
//!
//! What's here is the calibration machinery itself:
//! [`ContentionModel`] (turns recorded per-reserve win/loss observations
//! into `contention(reserve)`) and [`TipCurve::calibrate`] (fits `k` from
//! a set of [`ObserveRecord`]s). Both are fully implemented and unit
//! tested against synthetic observation data — proving the *math* is
//! correct — but [`TipCurve::calibrate`] refuses to produce a curve from
//! zero or too little data ([`CalibrationError::InsufficientData`])
//! rather than silently falling back to a made-up constant. Until a real
//! `observe` run exists to calibrate against, `strategy::tip::static_tip_bid`
//! (the Stage B floor) remains the correct function to call.

use std::collections::HashMap;

use gyrfalcon_core::Pubkey;

/// One recorded outcome from an `observe`-mode session: for a liquidation
/// candidate the engine detected but did not submit, what would its bid
/// have been, and — inferred from whether a competing bot's transaction
/// landed on that position in that slot — would it plausibly have won?
/// `docs/STRATEGY.md#cold-start`: "records what the engine *would* have
/// bid and whether a competing bot's landed transaction implies it would
/// have won."
#[derive(Debug, Clone, Copy)]
pub struct ObserveRecord {
    pub reserve: Pubkey,
    /// Whether a competitor landed a liquidation on this same position in
    /// this slot before the engine's own would-have-fired transaction
    /// would have. `true` = lock contested and lost.
    pub lock_contested: bool,
    /// The `floor_tip + k * contention * (r*b)` bid the engine would have
    /// placed, and the `r * b` (bonus) it was computed from — needed to
    /// invert the formula during calibration.
    pub would_have_bid_usd: f64,
    pub bonus_usd: f64,
}

/// Per-reserve empirical contention rate, `contention(reserve)` in Eq. 2b
/// — the same map STRATEGY.md says feeds both the tip curve and (per
/// `docs/RUNBOOK.md`) the router's depth checks.
#[derive(Debug, Default)]
pub struct ContentionModel {
    contested: HashMap<Pubkey, u32>,
    total: HashMap<Pubkey, u32>,
}

impl ContentionModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, reserve: Pubkey, lock_contested: bool) {
        *self.total.entry(reserve).or_insert(0) += 1;
        if lock_contested {
            *self.contested.entry(reserve).or_insert(0) += 1;
        }
    }

    /// `contention(reserve)` in `[0.0, 1.0]` — fraction of observed
    /// attempts on this reserve that were lock-contested. `0.0` (not an
    /// error) for a reserve with no observations, since an unobserved
    /// reserve contributes nothing to `k * contention * (r*b)` either way.
    pub fn contention(&self, reserve: Pubkey) -> f64 {
        let total = self.total.get(&reserve).copied().unwrap_or(0);
        if total == 0 {
            return 0.0;
        }
        let contested = self.contested.get(&reserve).copied().unwrap_or(0);
        contested as f64 / total as f64
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TipCurve {
    pub floor_tip_usd: f64,
    pub k: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    #[error(
        "cannot calibrate k from {0} observation(s) — need at least {MIN_OBSERVATIONS} \
         from a real observe-mode session; see this module's doc for why a fabricated \
         default is not an acceptable substitute"
    )]
    InsufficientData(usize),
}

const MIN_OBSERVATIONS: usize = 30;

impl TipCurve {
    /// Fit `k` from recorded observations and a contention model built
    /// from the same data. Method: for each record, invert
    /// `bid = floor_tip + k * contention * bonus` to solve for the
    /// smallest `k` implied by that single observation, then take the
    /// median across all records — median rather than mean so a handful
    /// of outlier slots (e.g. a one-off gas spike) don't dominate the fit,
    /// consistent with `docs/STRATEGY.md`'s framing of `k` as a stable,
    /// periodically-recalibrated constant rather than something reactive
    /// to any single slot.
    pub fn calibrate(
        records: &[ObserveRecord],
        floor_tip_usd: f64,
    ) -> Result<Self, CalibrationError> {
        if records.len() < MIN_OBSERVATIONS {
            return Err(CalibrationError::InsufficientData(records.len()));
        }

        let mut model = ContentionModel::new();
        for r in records {
            model.observe(r.reserve, r.lock_contested);
        }

        let mut implied_ks: Vec<f64> = records
            .iter()
            .filter_map(|r| {
                let contention = model.contention(r.reserve);
                let denom = contention * r.bonus_usd;
                if denom <= 0.0 {
                    return None; // uncontended reserve tells us nothing about k
                }
                let implied_k = (r.would_have_bid_usd - floor_tip_usd) / denom;
                Some(implied_k.max(0.0))
            })
            .collect();

        if implied_ks.is_empty() {
            return Err(CalibrationError::InsufficientData(0));
        }

        implied_ks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let k = implied_ks[implied_ks.len() / 2];

        Ok(TipCurve { floor_tip_usd, k })
    }

    /// The Eq. 2b bid, given this curve's calibrated `floor_tip`/`k`, a
    /// contention reading, and the risk-config bounds.
    pub fn bid(
        &self,
        contention: f64,
        bonus_usd: f64,
        risk: &gyrfalcon_config::RiskConfig,
        min_tip_usd: f64,
    ) -> f64 {
        let raw = self.floor_tip_usd + self.k * contention * bonus_usd;
        let ceiling = (risk.max_tip_pct_of_bonus * bonus_usd).min(risk.max_tip_per_tx_usd);
        raw.clamp(min_tip_usd, ceiling.max(min_tip_usd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new([b; 32])
    }

    #[test]
    fn contention_model_computes_fraction_contested() {
        let mut model = ContentionModel::new();
        let reserve = pk(1);
        model.observe(reserve, true);
        model.observe(reserve, true);
        model.observe(reserve, false);
        model.observe(reserve, false);
        assert_eq!(model.contention(reserve), 0.5);
    }

    #[test]
    fn unobserved_reserve_has_zero_contention_not_an_error() {
        let model = ContentionModel::new();
        assert_eq!(model.contention(pk(99)), 0.0);
    }

    #[test]
    fn calibration_refuses_too_few_observations() {
        let records = vec![ObserveRecord {
            reserve: pk(1),
            lock_contested: true,
            would_have_bid_usd: 5.0,
            bonus_usd: 40.0,
        }];
        let result = TipCurve::calibrate(&records, 0.0);
        assert!(matches!(result, Err(CalibrationError::InsufficientData(1))));
    }

    #[test]
    fn calibration_recovers_a_known_k_from_synthetic_data() {
        // Synthetic data generated FROM a known k=0.5, floor=1.0, so the
        // fit should recover ~0.5 -- proves the calibration math itself
        // is correct, independent of ever having real observe data. Not
        // shipped as a real k (see module doc).
        let true_k = 0.5;
        let floor = 1.0;
        let mut records = Vec::new();
        let reserve_a = pk(1);
        let reserve_b = pk(2);

        // reserve_a: 70% contended, reserve_b: 20% contended.
        for i in 0..40 {
            let contested_a = i % 10 < 7;
            let bonus = 40.0;
            records.push(ObserveRecord {
                reserve: reserve_a,
                lock_contested: contested_a,
                bonus_usd: bonus,
                would_have_bid_usd: floor + true_k * 0.7 * bonus,
            });
        }
        for i in 0..40 {
            let contested_b = i % 10 < 2;
            let bonus = 25.0;
            records.push(ObserveRecord {
                reserve: reserve_b,
                lock_contested: contested_b,
                bonus_usd: bonus,
                would_have_bid_usd: floor + true_k * 0.2 * bonus,
            });
        }

        let curve = TipCurve::calibrate(&records, floor).expect("enough data to calibrate");
        assert!(
            (curve.k - true_k).abs() < 0.05,
            "expected k close to {true_k}, got {}",
            curve.k
        );
    }

    #[test]
    fn bid_respects_ceiling_even_with_high_contention() {
        let curve = TipCurve {
            floor_tip_usd: 1.0,
            k: 10.0, // deliberately aggressive
        };
        let risk = gyrfalcon_config::RiskConfig {
            min_profit_usd: 5.0,
            min_tip_usd: 0.0,
            sync_lag_halt_slots: 3,
            contention_ceiling: 0.8,
            max_tip_per_tx_usd: 50.0,
            max_tip_per_slot_usd: 600.0,
            max_tip_pct_of_bonus: 0.4,
            max_drawdown_usd: 1000.0,
            consecutive_revert_limit: 3,
        };
        let bid = curve.bid(1.0, 1000.0, &risk, 0.0); // huge bonus, max contention
        assert!(bid <= 50.0);
    }
}
