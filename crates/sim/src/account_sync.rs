//! Continuous account-sync pipeline — keeps a simulated account set
//! current to the slot, per `docs/ARCHITECTURE.md#simulation`'s framing of
//! this as the system's single point of failure: a simulation run against
//! stale accounts produces a confidently wrong answer, not a visible
//! error.
//!
//! Scoped to Kamino accounts only for Stage B (`docs/BUILD_ORDER.md` item
//! 7). This module is deliberately protocol-agnostic at the storage layer
//! — it holds raw `(Pubkey -> (data, slot))` pairs — with protocol-specific
//! decoding staying in `gyrfalcon-health`'s adapters, so Save/MarginFi
//! extend this in Stage C by feeding it their own accounts, not by
//! rewriting it.

use gyrfalcon_core::Pubkey;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct SlottedAccount {
    data: Vec<u8>,
    slot: u64,
}

/// A simulated account set, kept current by repeated calls to
/// [`AccountSyncPipeline::apply_update`] as new account states arrive.
#[derive(Debug, Default)]
pub struct AccountSyncPipeline {
    accounts: HashMap<Pubkey, SlottedAccount>,
    /// Highest slot any account update has carried — the pipeline's own
    /// notion of "now."
    current_slot: u64,
}

impl AccountSyncPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one account update. Out-of-order updates for the same
    /// pubkey (an update at an older slot arriving after a newer one) are
    /// dropped rather than silently overwriting fresher state — this is
    /// exactly the kind of staleness bug `ARCHITECTURE.md` warns produces
    /// a wrong answer with no visible error, so it's guarded here instead
    /// of trusted to arrival order.
    pub fn apply_update(&mut self, pubkey: Pubkey, data: Vec<u8>, slot: u64) -> bool {
        self.current_slot = self.current_slot.max(slot);
        match self.accounts.get(&pubkey) {
            Some(existing) if existing.slot > slot => false,
            _ => {
                self.accounts.insert(pubkey, SlottedAccount { data, slot });
                true
            }
        }
    }

    pub fn get(&self, pubkey: &Pubkey) -> Option<&[u8]> {
        self.accounts.get(pubkey).map(|a| a.data.as_slice())
    }

    pub fn account_slot(&self, pubkey: &Pubkey) -> Option<u64> {
        self.accounts.get(pubkey).map(|a| a.slot)
    }

    pub fn current_slot(&self) -> u64 {
        self.current_slot
    }

    /// How far behind `current_slot` this specific account's last update
    /// is. A simulation run should refuse to trust an account whose lag
    /// exceeds the configured `sync_lag_halt_slots` breaker
    /// (`docs/CONFIGURATION.md`) — that check lives in `strategy`/`sim`'s
    /// caller, this just exposes the number.
    pub fn account_lag(&self, pubkey: &Pubkey) -> Option<u64> {
        self.accounts
            .get(pubkey)
            .map(|a| self.current_slot.saturating_sub(a.slot))
    }

    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new([b; 32])
    }

    #[test]
    fn newer_update_overwrites_older() {
        let mut pipeline = AccountSyncPipeline::new();
        assert!(pipeline.apply_update(pk(1), vec![1], 100));
        assert!(pipeline.apply_update(pk(1), vec![2], 200));
        assert_eq!(pipeline.get(&pk(1)), Some(&[2u8][..]));
        assert_eq!(pipeline.account_slot(&pk(1)), Some(200));
    }

    #[test]
    fn out_of_order_older_update_is_dropped_not_applied() {
        let mut pipeline = AccountSyncPipeline::new();
        pipeline.apply_update(pk(1), vec![2], 200);
        let applied = pipeline.apply_update(pk(1), vec![1], 100);
        assert!(!applied);
        assert_eq!(
            pipeline.get(&pk(1)),
            Some(&[2u8][..]),
            "stale update must not overwrite fresher state"
        );
    }

    #[test]
    fn account_lag_reflects_distance_from_pipeline_current_slot() {
        let mut pipeline = AccountSyncPipeline::new();
        pipeline.apply_update(pk(1), vec![1], 100);
        pipeline.apply_update(pk(2), vec![1], 150); // advances current_slot to 150
        assert_eq!(pipeline.account_lag(&pk(1)), Some(50));
        assert_eq!(pipeline.account_lag(&pk(2)), Some(0));
    }
}
