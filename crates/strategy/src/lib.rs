//! Sizing, arbitration, tip bidding, treasury checks, and circuit
//! breakers — the deterministic decision layer described in
//! `docs/STRATEGY.md`. Deliberately one module tree, not a trait: per
//! `docs/API.md#trait-contracts`, strategy is invoked at three pipeline
//! stages rather than implemented per-protocol.
//!
//! Stage B item 8 (minimum viable) scope, per `docs/BUILD_ORDER.md`:
//! close-factor sizing + a static tip floor. The dynamic contention-scaled
//! tip curve (Whitepaper Eq. 2b) is Stage C item 15, gated on `observe`-mode
//! data this build cannot collect — see `tip.rs`'s module doc. Treasury
//! checks and circuit breakers (Stage B items 11 and part of the
//! retry/backoff section) are wired in now rather than bolted on later,
//! per BUILD_ORDER.md item 11's explicit instruction.

pub mod arbitration;
pub mod breakers;
pub mod dynamic_tip;
pub mod sizing;
pub mod tip;
pub mod treasury_check;

pub use arbitration::{arbitrate, ArbitratedCandidate, ArbitrationOutcome};
pub use breakers::{BreakerCheck, BreakerReason, BreakerState, RouteKey};
pub use dynamic_tip::{CalibrationError, ContentionModel, ObserveRecord, TipCurve};
pub use sizing::{
    size_and_route, size_position, size_position_stepped, size_position_with_feasibility,
    BindingConstraint, DynamicRoute, GasRegime, RouteFeasibilityProvider, SizingDecision,
    StaticRoute, MAX_TX_CU,
};
pub use tip::{static_tip_bid, STATIC_TIP_FLOOR_USD};
pub use treasury_check::{SlotBudget, TreasuryDecision};
