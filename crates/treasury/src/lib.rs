//! Hot wallet accounting inputs: realized-PnL ledger (feeds the drawdown
//! breaker) and sweep scheduling / minimum-balance checks. See
//! `docs/ARCHITECTURE.md#treasury` and
//! `docs/STRATEGY.md#treasury-and-capital-management`.
//!
//! Neither module here queries a real wallet balance or submits a real
//! sweep transaction — see `sweep.rs`'s module doc for why (no live RPC
//! access in this build) and what Stage B+/C needs to add.

pub mod pnl_ledger;
pub mod sweep;

pub use pnl_ledger::{drawdown_breached, PnlLedger};
pub use sweep::{
    is_below_minimum_balance, parse_sweep_interval_slots, sweep_due, SweepIntervalError,
};
