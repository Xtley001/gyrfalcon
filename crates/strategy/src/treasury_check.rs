//! Treasury exposure checks — `docs/STRATEGY.md#treasury-and-capital-management`,
//! wired into strategy from the start per `docs/BUILD_ORDER.md` item 11's
//! instruction ("wired into strategy's arbitration step from the start
//! rather than bolted on later").
//!
//! This module tracks per-slot committed tip spend and answers "is this
//! candidate's tip within budget," and separately "is the treasury above
//! its minimum operating balance." It does not move funds or talk to a
//! wallet — that's `gyrfalcon-treasury`'s job (balance queries, sweeps);
//! this is the check strategy runs against numbers `treasury` supplies.

use gyrfalcon_config::{RiskConfig, TreasuryConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreasuryDecision {
    Approved,
    /// This candidate's own tip exceeds `risk.max_tip_per_tx_usd`.
    ExceedsPerTxCap,
    /// Firing this candidate would push the slot's committed spend past
    /// `risk.max_tip_per_slot_usd`; STRATEGY.md: "queue for the next slot
    /// instead of firing uncapped."
    ExceedsPerSlotBudget,
    /// Treasury balance is below `treasury.min_balance_sol` — STRATEGY.md:
    /// "the engine stops firing new candidates and alerts."
    BelowMinimumBalance,
}

/// Tracks committed tip spend within one slot. A new instance per slot —
/// STRATEGY.md's per-slot cap resets every slot, it does not accumulate
/// across slots (that's `risk.max_drawdown_usd`'s job, a different
/// breaker).
#[derive(Debug, Default)]
pub struct SlotBudget {
    committed_usd: f64,
}

impl SlotBudget {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn committed_usd(&self) -> f64 {
        self.committed_usd
    }

    /// Checks a candidate's proposed tip against every treasury-side
    /// constraint, in the order STRATEGY.md's arbitration section lists
    /// them: per-tx cap, then per-slot budget, then treasury floor.
    /// Returns the first failing check, or `Approved` if all pass. Does
    /// **not** commit the spend — call [`SlotBudget::commit`] once the
    /// candidate is actually fired, since a candidate that fails a later
    /// arbitration step (e.g. loses a reserve-conflict tiebreak) should
    /// never have reserved slot budget it didn't use.
    pub fn check(
        &self,
        tip_usd: f64,
        current_balance_sol: f64,
        risk: &RiskConfig,
        treasury: &TreasuryConfig,
    ) -> TreasuryDecision {
        if current_balance_sol < treasury.min_balance_sol {
            return TreasuryDecision::BelowMinimumBalance;
        }
        if tip_usd > risk.max_tip_per_tx_usd {
            return TreasuryDecision::ExceedsPerTxCap;
        }
        if self.committed_usd + tip_usd > risk.max_tip_per_slot_usd {
            return TreasuryDecision::ExceedsPerSlotBudget;
        }
        TreasuryDecision::Approved
    }

    /// Commit a tip spend against this slot's budget. Call only after
    /// [`SlotBudget::check`] returned `Approved` and the candidate has
    /// actually been fired.
    pub fn commit(&mut self, tip_usd: f64) {
        self.committed_usd += tip_usd;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn treasury(min_balance_sol: f64) -> TreasuryConfig {
        TreasuryConfig {
            wallet_path: "/tmp/wallet.json".to_string(),
            min_balance_sol,
            sweep_interval: "1h".to_string(),
        }
    }

    #[test]
    fn approves_a_candidate_comfortably_within_all_caps() {
        let budget = SlotBudget::new();
        let decision = budget.check(50.0, 5.0, &risk(), &treasury(1.0));
        assert_eq!(decision, TreasuryDecision::Approved);
    }

    #[test]
    fn rejects_below_minimum_treasury_balance_before_anything_else() {
        let budget = SlotBudget::new();
        // Tip is fine on its own, but balance is below floor.
        let decision = budget.check(10.0, 0.5, &risk(), &treasury(1.0));
        assert_eq!(decision, TreasuryDecision::BelowMinimumBalance);
    }

    #[test]
    fn rejects_tip_exceeding_per_tx_cap_regardless_of_bonus_size() {
        let budget = SlotBudget::new();
        let decision = budget.check(200.0, 5.0, &risk(), &treasury(1.0));
        assert_eq!(decision, TreasuryDecision::ExceedsPerTxCap);
    }

    #[test]
    fn rejects_once_committed_spend_plus_new_tip_exceeds_slot_budget() {
        let mut budget = SlotBudget::new();
        budget.commit(580.0);
        let decision = budget.check(50.0, 5.0, &risk(), &treasury(1.0));
        assert_eq!(decision, TreasuryDecision::ExceedsPerSlotBudget);
    }

    #[test]
    fn commit_accumulates_across_multiple_candidates_in_the_same_slot() {
        let mut budget = SlotBudget::new();
        budget.commit(100.0);
        budget.commit(200.0);
        assert_eq!(budget.committed_usd(), 300.0);
    }
}
