//! Multi-candidate arbitration — `docs/STRATEGY.md#multi-candidate-arbitration`.
//!
//! Implements steps 1-3 (rank by EV, reserve-conflict check, treasury
//! budget check) for one evaluation window. Step 4 ("fire remaining
//! candidates independently") is a statement about *not* merging
//! candidates into one transaction — nothing to implement here, since this
//! module only decides order/admission, never transaction assembly
//! (`bundler`'s job).

use crate::treasury_check::{SlotBudget, TreasuryDecision};
use gyrfalcon_config::{RiskConfig, TreasuryConfig};
use gyrfalcon_core::{Pubkey, RoutedCandidate};

#[derive(Debug, Clone, PartialEq)]
pub enum ArbitrationOutcome {
    /// Fire this slot. Carries the tip that was committed against the
    /// slot budget so the caller doesn't have to recompute it.
    Fire { tip_usd: f64 },
    /// Deprioritized to next slot — lost a reserve-conflict tiebreak
    /// against a higher-EV candidate sharing the same source reserve.
    DeferredReserveConflict,
    /// Deprioritized to next slot — treasury/slot budget exhausted before
    /// reaching this candidate in EV-descending order.
    DeferredBudget(TreasuryDecision),
    /// Below `risk.min_profit_usd` — discarded, not deferred (STRATEGY.md:
    /// "Scored --> Discarded: EV <= risk.min_profit_usd").
    Discarded,
}

#[derive(Debug, Clone)]
pub struct ArbitratedCandidate {
    pub routed: RoutedCandidate,
    pub outcome: ArbitrationOutcome,
}

/// Arbitrate one evaluation window's worth of already-routed, already-
/// scored candidates. `tip_for` computes step 3's committed spend for a
/// candidate (`crate::tip::static_tip_bid` in Stage B) — passed in rather
/// than hardcoded so this function doesn't hold an opinion on which tip
/// model is active.
pub fn arbitrate(
    mut candidates: Vec<RoutedCandidate>,
    risk: &RiskConfig,
    treasury_config: &TreasuryConfig,
    treasury_balance_sol: f64,
    tip_for: impl Fn(&RoutedCandidate) -> f64,
) -> Vec<ArbitratedCandidate> {
    // Step 1: rank by net EV descending.
    candidates.sort_by(|a, b| {
        b.expected
            .net_usd
            .partial_cmp(&a.expected.net_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut results = Vec::with_capacity(candidates.len());
    let mut locked_reserves: Vec<Pubkey> = Vec::new();
    let mut budget = SlotBudget::new();

    for routed in candidates {
        if routed.expected.net_usd <= risk.min_profit_usd {
            results.push(ArbitratedCandidate {
                routed,
                outcome: ArbitrationOutcome::Discarded,
            });
            continue;
        }

        // Step 2: reserve-conflict check. Since candidates are processed
        // in EV-descending order, the first candidate to touch a reserve
        // in this slot wins it; any later (lower-EV) candidate on the
        // same reserve is deferred.
        if locked_reserves.contains(&routed.flash_source.reserve) {
            results.push(ArbitratedCandidate {
                routed,
                outcome: ArbitrationOutcome::DeferredReserveConflict,
            });
            continue;
        }

        // Step 3: treasury budget check.
        let tip_usd = tip_for(&routed);
        let decision = budget.check(tip_usd, treasury_balance_sol, risk, treasury_config);
        if decision != TreasuryDecision::Approved {
            results.push(ArbitratedCandidate {
                routed,
                outcome: ArbitrationOutcome::DeferredBudget(decision),
            });
            continue;
        }

        budget.commit(tip_usd);
        locked_reserves.push(routed.flash_source.reserve);
        results.push(ArbitratedCandidate {
            routed,
            outcome: ArbitrationOutcome::Fire { tip_usd },
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::{BreachCandidate, FlashSource, ProfitEstimate, Protocol};

    fn risk() -> RiskConfig {
        RiskConfig {
            min_profit_usd: 5.0,
            min_tip_usd: 0.0,
            sync_lag_halt_slots: 3,
            contention_ceiling: 0.8,
            max_tip_per_tx_usd: 150.0,
            max_tip_per_slot_usd: 600.0,
            max_tip_pct_of_bonus: 0.4,
            max_drawdown_usd: 1000.0,
            consecutive_revert_limit: 3,
        }
    }

    fn treasury_config() -> TreasuryConfig {
        TreasuryConfig {
            wallet_path: "/tmp/wallet.json".to_string(),
            min_balance_sol: 1.0,
            sweep_interval: "1h".to_string(),
        }
    }

    fn candidate(reserve_byte: u8, net_usd: f64) -> RoutedCandidate {
        RoutedCandidate {
            candidate: BreachCandidate {
                protocol: Protocol::Kamino,
                position_id: Pubkey::new([reserve_byte; 32]),
                collateral_mint: Pubkey::new([1u8; 32]),
                debt_mint: Pubkey::new([2u8; 32]),
                health_factor: 0.9,
                close_factor_max_repay: 1_000_000,
                slot: 100,
            },
            repay_amount: 500_000,
            flash_source: FlashSource {
                protocol: Protocol::Kamino,
                reserve: Pubkey::new([reserve_byte; 32]),
                fee_bps: 5,
            },
            expected: ProfitEstimate {
                bonus_usd: 40.0,
                est_slippage_usd: 2.0,
                flash_fee_usd: 1.0,
                est_cu_cost_usd: 0.5,
                bid_tip_usd: 10.0,
                net_usd,
            },
        }
    }

    #[test]
    fn ranks_and_fires_all_when_no_conflicts_or_budget_pressure() {
        let candidates = vec![candidate(1, 10.0), candidate(2, 30.0), candidate(3, 20.0)];
        let results = arbitrate(candidates, &risk(), &treasury_config(), 5.0, |_| 10.0);

        assert_eq!(results.len(), 3);
        // EV-descending: 30, 20, 10.
        assert_eq!(results[0].routed.expected.net_usd, 30.0);
        assert_eq!(results[1].routed.expected.net_usd, 20.0);
        assert_eq!(results[2].routed.expected.net_usd, 10.0);
        for r in &results {
            assert!(matches!(r.outcome, ArbitrationOutcome::Fire { .. }));
        }
    }

    #[test]
    fn discards_candidates_at_or_below_min_profit() {
        let candidates = vec![candidate(1, 5.0), candidate(2, 4.9)];
        let results = arbitrate(candidates, &risk(), &treasury_config(), 5.0, |_| 1.0);
        assert!(results
            .iter()
            .all(|r| r.outcome == ArbitrationOutcome::Discarded));
    }

    #[test]
    fn lower_ev_candidate_on_same_reserve_is_deferred_not_fired() {
        // Same reserve byte (1) for both -> conflict.
        let candidates = vec![candidate(1, 30.0), candidate(1, 20.0)];
        let results = arbitrate(candidates, &risk(), &treasury_config(), 5.0, |_| 10.0);

        assert!(matches!(
            results[0].outcome,
            ArbitrationOutcome::Fire { .. }
        ));
        assert_eq!(
            results[1].outcome,
            ArbitrationOutcome::DeferredReserveConflict
        );
    }

    #[test]
    fn exhausted_slot_budget_defers_remaining_candidates() {
        let candidates = vec![candidate(1, 30.0), candidate(2, 25.0), candidate(3, 20.0)];
        // Each candidate "costs" 250 in tip; the default per-tx cap (150)
        // would block that on its own, so raise it for this test — the
        // point here is the smaller 600 per-slot cap, not the per-tx one.
        let mut r = risk();
        r.max_tip_per_tx_usd = 300.0;
        let results = arbitrate(candidates, &r, &treasury_config(), 5.0, |_| 250.0);

        assert!(matches!(
            results[0].outcome,
            ArbitrationOutcome::Fire { .. }
        ));
        assert!(matches!(
            results[1].outcome,
            ArbitrationOutcome::Fire { .. }
        ));
        assert!(matches!(
            results[2].outcome,
            ArbitrationOutcome::DeferredBudget(_)
        ));
    }
}
