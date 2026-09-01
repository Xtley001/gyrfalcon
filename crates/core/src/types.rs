//! Core pipeline data types, verbatim from [`docs/API.md`](../../../docs/API.md#core-data-types).
//!
//! This module is the single source of truth for the shapes that cross
//! crate boundaries over the internal event bus (ingestion -> health ->
//! strategy -> sim -> bundler -> submit -> store). If a field here and
//! `docs/API.md` disagree, the doc wins until updated in the same PR
//! (see `CONTRIBUTING.md`).

use crate::protocol::{Protocol, RevertReason};
use crate::pubkey::Pubkey;
use serde::{Deserialize, Serialize};

/// Emitted by a health adapter when a position crosses its threshold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreachCandidate {
    pub protocol: Protocol,
    pub position_id: Pubkey,
    pub collateral_mint: Pubkey,
    pub debt_mint: Pubkey,
    /// < 1.0
    pub health_factor: f64,
    /// Protocol-enforced ceiling, base units.
    pub close_factor_max_repay: u64,
    pub slot: u64,
}

/// A flash-loan source selected by the router for a given mint/amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashSource {
    pub protocol: Protocol,
    pub reserve: Pubkey,
    /// Flash-borrow fee in basis points at selection time.
    pub fee_bps: u32,
}

/// Output of `strategy::size_and_route`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutedCandidate {
    pub candidate: BreachCandidate,
    /// <= candidate.close_factor_max_repay, per docs/STRATEGY.md sizing.
    pub repay_amount: u64,
    pub flash_source: FlashSource,
    /// Pi components, pre-simulation.
    pub expected: ProfitEstimate,
}

/// Whitepaper Eq. 2 components.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProfitEstimate {
    pub bonus_usd: f64,
    pub est_slippage_usd: f64,
    pub flash_fee_usd: f64,
    pub est_cu_cost_usd: f64,
    pub bid_tip_usd: f64,
    /// Whitepaper Eq. 2.
    pub net_usd: f64,
}

/// Output of `sim::simulate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimResult {
    pub routed: RoutedCandidate,
    /// Eqs 4-6 all hold.
    pub feasible: bool,
    pub cu_measured: u32,
    pub tx_bytes: usize,
    /// net_usd > risk.min_profit_usd, re-checked post-sim.
    pub profitable: bool,
}

/// Output of `bundler::build`.
///
/// `versioned_tx` is opaque bytes at this stage rather than
/// `solana_sdk::VersionedTransaction` — see `crates/core/src/pubkey.rs` for
/// why the heavier Solana SDK dependency is deferred to Stage B, when
/// `bundler` starts assembling real v0 transactions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    pub sim: SimResult,
    pub versioned_tx: Vec<u8>,
    pub alt_keys: Vec<Pubkey>,
}

/// Output of `submit::send`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SubmitOutcome {
    Landed {
        slot: u64,
        actual_net_usd: f64,
    },
    Reverted {
        reason: RevertReason,
    },
    /// Neither path included it before the position was resolved elsewhere.
    NotIncluded,
}

/// Persisted to `store` on every terminal outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiquidationRecord {
    pub routed: RoutedCandidate,
    pub outcome: SubmitOutcome,
    pub submitted_at_slot: u64,
    pub resolved_at_slot: u64,
}
