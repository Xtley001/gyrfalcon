//! Trait contracts, verbatim from [`docs/API.md`](../../../docs/API.md#trait-contracts).
//!
//! Every protocol adapter implements `HealthAdapter` so the shared framework
//! in `health` never special-cases a protocol by name outside its own
//! adapter module. `strategy` is deliberately not a trait here — per API.md
//! it is one deterministic module (`crates/strategy`) implementing sizing,
//! arbitration, tip bidding, and treasury checks exactly as specified in
//! `docs/STRATEGY.md`.

use crate::pubkey::Pubkey;
use crate::types::{Bundle, RoutedCandidate, SimResult, SubmitOutcome};

/// One raw account update off the Geyser ring buffer, decoded far enough to
/// route to the right protocol adapter but not yet protocol-specific.
#[derive(Debug, Clone)]
pub struct AccountUpdate {
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub data: Vec<u8>,
    pub slot: u64,
}

pub trait HealthAdapter {
    /// Decode one account update; return a candidate iff it now breaches.
    fn on_account_update(&mut self, update: AccountUpdate)
        -> Option<crate::types::BreachCandidate>;

    /// Protocol-specific close factor for a given position.
    fn close_factor(&self, position_id: Pubkey) -> u64;

    /// Current count of tracked positions, for the dashboard / stat band.
    fn position_count(&self) -> usize;

    /// Slots since this adapter's account-sync pipeline last confirmed
    /// current. Feeds the sync-lag circuit breaker (Invariant I2).
    fn sync_lag_slots(&self) -> u64;
}

pub trait FlashSourceRouter {
    /// Whitepaper Eq. 3 — select a source reserve for a given mint and
    /// amount.
    fn route(&self, mint: Pubkey, amount: u64) -> Option<crate::types::FlashSource>;
}

pub trait Simulator {
    /// Runs the candidate against a slot-current account set. Never mutates
    /// real state.
    fn simulate(&self, routed: RoutedCandidate) -> SimResult;
}

#[async_trait::async_trait]
pub trait Submitter {
    /// Fires both paths in parallel; returns once either resolves or a
    /// timeout elapses.
    async fn submit(&self, bundle: Bundle) -> SubmitOutcome;
}
