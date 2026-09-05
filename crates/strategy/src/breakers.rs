//! Circuit breakers — `docs/STRATEGY.md#circuit-breakers`.
//!
//! Four breakers, three scopes (whole engine, one protocol, one route).
//! `docs/STRATEGY.md`: "A breaker halting the whole engine is a
//! deliberately higher bar than halting one adapter... (Invariant I4)" —
//! this module keeps that distinction structural: [`BreakerState::allows`]
//! takes a `Protocol` and a route key precisely so a whole-engine halt and
//! a single-route halt can never be conflated by a caller forgetting to
//! pass scope.

use gyrfalcon_core::Protocol;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerReason {
    ConsecutiveRevert,
    TreasuryFloor,
    Drawdown,
    SyncLag,
}

/// A route key for the consecutive-revert breaker — STRATEGY.md scopes
/// this breaker to "that route only," where a route is (protocol,
/// position, flash reserve), not just a position, since the same position
/// could in principle be routed through a different reserve on retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouteKey {
    pub protocol: Protocol,
    pub position_id: gyrfalcon_core::Pubkey,
    pub flash_reserve: gyrfalcon_core::Pubkey,
}

#[derive(Debug, Default)]
pub struct BreakerState {
    /// Whole-engine halt. Manual re-arm required (STRATEGY.md's Reset
    /// column) — set back to `false` only by an explicit
    /// [`BreakerState::rearm_drawdown`] call, never automatically.
    drawdown_tripped: bool,
    /// Whole-engine halt, automatic reset once balance clears the floor
    /// again (STRATEGY.md's Reset column) — tracked as a plain flag the
    /// caller updates from a live balance check, not derived internally
    /// (this module doesn't talk to a wallet).
    treasury_below_floor: bool,
    /// Per-protocol halt, automatic reset once sync lag clears
    /// (STRATEGY.md's Reset column) — same "caller-updated flag" pattern
    /// as `treasury_below_floor`.
    protocols_halted_for_sync_lag: HashMap<Protocol, bool>,
    consecutive_reverts: HashMap<RouteKey, u32>,
    /// Manual re-arm required per STRATEGY.md; a route that hit the limit
    /// stays out of rotation until explicitly cleared.
    routes_taken_out_of_rotation: HashMap<RouteKey, bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerCheck {
    Allowed,
    Halted(BreakerReason),
}

impl BreakerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a revert on `route`. If this pushes the route to
    /// `consecutive_revert_limit` in a row, it's taken out of rotation.
    /// Any successful land (call [`BreakerState::record_success`] instead)
    /// resets the counter to zero, per "consecutive."
    pub fn record_revert(&mut self, route: RouteKey, consecutive_revert_limit: u32) {
        let count = self.consecutive_reverts.entry(route).or_insert(0);
        *count += 1;
        if *count >= consecutive_revert_limit {
            self.routes_taken_out_of_rotation.insert(route, true);
        }
    }

    pub fn record_success(&mut self, route: RouteKey) {
        self.consecutive_reverts.insert(route, 0);
        self.routes_taken_out_of_rotation.remove(&route);
    }

    /// Manual re-arm after root-causing a revert streak (STRATEGY.md's
    /// Reset column for the consecutive-revert breaker).
    pub fn rearm_route(&mut self, route: RouteKey) {
        self.consecutive_reverts.insert(route, 0);
        self.routes_taken_out_of_rotation.remove(&route);
    }

    /// Re-arm all disabled routes across the engine.
    pub fn rearm_all_routes(&mut self) {
        self.consecutive_reverts.clear();
        self.routes_taken_out_of_rotation.clear();
    }

    pub fn set_treasury_below_floor(&mut self, below_floor: bool) {
        self.treasury_below_floor = below_floor;
    }

    pub fn set_drawdown_tripped(&mut self) {
        self.drawdown_tripped = true;
    }

    /// Manual re-arm (STRATEGY.md's Reset column for the drawdown breaker).
    pub fn rearm_drawdown(&mut self) {
        self.drawdown_tripped = false;
    }

    pub fn set_protocol_sync_lag_halted(&mut self, protocol: Protocol, halted: bool) {
        self.protocols_halted_for_sync_lag.insert(protocol, halted);
    }

    /// The single entry point every submission decision should go
    /// through: is this specific (protocol, route) combination currently
    /// allowed to fire, checking all four breakers in STRATEGY.md's table
    /// order (whole-engine breakers first, since they're the higher bar).
    pub fn allows(&self, protocol: Protocol, route: RouteKey) -> BreakerCheck {
        if self.drawdown_tripped {
            return BreakerCheck::Halted(BreakerReason::Drawdown);
        }
        if self.treasury_below_floor {
            return BreakerCheck::Halted(BreakerReason::TreasuryFloor);
        }
        if self
            .protocols_halted_for_sync_lag
            .get(&protocol)
            .copied()
            .unwrap_or(false)
        {
            return BreakerCheck::Halted(BreakerReason::SyncLag);
        }
        if self
            .routes_taken_out_of_rotation
            .get(&route)
            .copied()
            .unwrap_or(false)
        {
            return BreakerCheck::Halted(BreakerReason::ConsecutiveRevert);
        }
        BreakerCheck::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::Pubkey;

    fn route(byte: u8) -> RouteKey {
        RouteKey {
            protocol: Protocol::Kamino,
            position_id: Pubkey::new([byte; 32]),
            flash_reserve: Pubkey::new([byte + 1; 32]),
        }
    }

    #[test]
    fn allows_by_default() {
        let state = BreakerState::new();
        assert_eq!(
            state.allows(Protocol::Kamino, route(1)),
            BreakerCheck::Allowed
        );
    }

    #[test]
    fn consecutive_reverts_take_only_that_route_out_of_rotation() {
        let mut state = BreakerState::new();
        let r1 = route(1);
        let r2 = route(2);

        state.record_revert(r1, 3);
        state.record_revert(r1, 3);
        assert_eq!(state.allows(Protocol::Kamino, r1), BreakerCheck::Allowed); // 2 reverts, limit 3
        state.record_revert(r1, 3);
        assert_eq!(
            state.allows(Protocol::Kamino, r1),
            BreakerCheck::Halted(BreakerReason::ConsecutiveRevert)
        );
        // Invariant I4: other route unaffected.
        assert_eq!(state.allows(Protocol::Kamino, r2), BreakerCheck::Allowed);
    }

    #[test]
    fn a_success_resets_the_consecutive_counter() {
        let mut state = BreakerState::new();
        let r = route(1);
        state.record_revert(r, 3);
        state.record_revert(r, 3);
        state.record_success(r);
        state.record_revert(r, 3);
        // Only 1 revert since the reset -> still allowed.
        assert_eq!(state.allows(Protocol::Kamino, r), BreakerCheck::Allowed);
    }

    #[test]
    fn sync_lag_halts_only_the_affected_protocol() {
        let mut state = BreakerState::new();
        state.set_protocol_sync_lag_halted(Protocol::Kamino, true);
        assert_eq!(
            state.allows(Protocol::Kamino, route(1)),
            BreakerCheck::Halted(BreakerReason::SyncLag)
        );
        state.set_protocol_sync_lag_halted(Protocol::Kamino, false);
        assert_eq!(
            state.allows(Protocol::Kamino, route(1)),
            BreakerCheck::Allowed
        );
    }

    #[test]
    fn drawdown_halts_the_whole_engine_regardless_of_protocol() {
        let mut state = BreakerState::new();
        state.set_drawdown_tripped();
        assert_eq!(
            state.allows(Protocol::Kamino, route(1)),
            BreakerCheck::Halted(BreakerReason::Drawdown)
        );
        assert_eq!(
            state.allows(Protocol::Kamino, route(2)),
            BreakerCheck::Halted(BreakerReason::Drawdown)
        );
    }

    #[test]
    fn drawdown_requires_manual_rearm_not_automatic_reset() {
        let mut state = BreakerState::new();
        state.set_drawdown_tripped();
        state.rearm_drawdown();
        assert_eq!(
            state.allows(Protocol::Kamino, route(1)),
            BreakerCheck::Allowed
        );
    }

    #[test]
    fn treasury_floor_halts_whole_engine_and_clears_automatically() {
        let mut state = BreakerState::new();
        state.set_treasury_below_floor(true);
        assert_eq!(
            state.allows(Protocol::Kamino, route(1)),
            BreakerCheck::Halted(BreakerReason::TreasuryFloor)
        );
        state.set_treasury_below_floor(false); // automatic once refilled
        assert_eq!(
            state.allows(Protocol::Kamino, route(1)),
            BreakerCheck::Allowed
        );
    }
}
