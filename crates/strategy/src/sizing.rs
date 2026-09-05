//! Position sizing — `docs/STRATEGY.md#position-sizing` & `04_STRATEGY_RISK.md §2`.
//!
//! "The engine repays the maximum allowed by the close factor... unless
//! flash-source depth or route feasibility reduces it."
//!
//! Route-feasibility stepping checks whether the resulting transaction fits
//! Solana's transaction limits:
//! - 1232-byte serialized transaction ceiling (`MAX_TX_BYTES`).
//! - 1.4M compute unit budget per transaction ceiling (`MAX_TX_CU`).
//!
//! If the route does not fit:
//! 1. First try: does using an Address Lookup Table (ALT) compact account keys enough to fit?
//! 2. If still infeasible even with ALTs: step the repay size down iteratively and re-check,
//!    logging every step-down so tip arbitration and risk telemetry know when feasibility,
//!    rather than close factor or flash depth, was the binding constraint.

use gyrfalcon_bundler::{AltManager, MAX_TX_BYTES};
use gyrfalcon_config::RiskConfig;
use gyrfalcon_core::types::{ProfitEstimate, RoutedCandidate};
use gyrfalcon_core::{BreachCandidate, FlashSourceRouter};
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::Instruction;
use solana_sdk::message::{v0, VersionedMessage};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;

/// Solana's hard ceiling for compute budget per transaction (1.4M CU).
pub const MAX_TX_CU: u32 = 1_400_000;

/// The binding constraint that determined the final position size.
///
/// DECISION MADE: Venue max-clip depth constraint (03_ROUTING_DEX.md §3) is folded
/// into the existing `FlashDepth` variant, keeping `BindingConstraint` a 4-variant enum
/// ({CloseFactor, FlashDepth, ByteLimit, ComputeBudget}) per 05_STRATEGY_RISK.md §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingConstraint {
    /// Bound by position's liquidation close factor ceiling.
    CloseFactor,
    /// Bound by available liquidity depth in the chosen flash source or DEX venue max-clip ceiling.
    FlashDepth,
    /// Bound by the 1232-byte serialized transaction limit.
    ByteLimit,
    /// Bound by the 1,400,000 compute unit transaction limit.
    ComputeBudget,
}

/// Gas regimes for transaction cost and minimum clip sizing per `01_PROTOCOLS.md §5`
/// and `05_STRATEGY_RISK.md §2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GasRegime {
    Normal,
    High,
    Spike,
}

impl GasRegime {
    /// Nominal transaction cost in USD for the regime.
    pub fn tx_cost_usd(&self) -> f64 {
        match self {
            GasRegime::Normal => 0.0015,
            GasRegime::High => 0.031,
            GasRegime::Spike => 0.14,
        }
    }

    /// Break-even debt in USD per `01_PROTOCOLS.md §5`.
    ///
    /// DECISION MADE: High regime has no populated break-even figure in source drop
    /// `01_PROTOCOLS.md §5`. Returns `None` rather than fabricating numbers per `05_STRATEGY_RISK.md §2`.
    pub fn break_even_debt_usd(&self) -> Option<f64> {
        match self {
            GasRegime::Normal => Some(0.02),
            GasRegime::High => None,
            GasRegime::Spike => Some(4.30),
        }
    }

    /// Minimum position clip in USD per `01_PROTOCOLS.md §5`.
    ///
    /// DECISION MADE: High regime has blank min-clip in `01_PROTOCOLS.md §5`. Per `05_STRATEGY_RISK.md §2`,
    /// we gate the High regime behind the conservative default of $250 min clip (matching Spike)
    /// until empirical production figures are collected, avoiding synthetic fabrication.
    pub fn min_clip_usd(&self) -> f64 {
        match self {
            GasRegime::Normal => 100.0,
            GasRegime::High => 250.0, // Conservative default matching Spike
            GasRegime::Spike => 250.0,
        }
    }

    /// Net profit at min clip in USD per `01_PROTOCOLS.md §5`.
    ///
    /// DECISION MADE: High regime has blank net-profit in source drop `01_PROTOCOLS.md §5`.
    /// Returns `None` rather than fabricating numbers per `05_STRATEGY_RISK.md §2`.
    pub fn net_profit_at_min_clip_usd(&self) -> Option<f64> {
        match self {
            GasRegime::Normal => Some(5.00),
            GasRegime::High => None,
            GasRegime::Spike => Some(12.28),
        }
    }
}

/// Sizing outcome containing the final size, ALT usage, and step-down metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct SizingDecision {
    /// Initial size computed from min(close_factor_max_repay, flash_source_available).
    pub initial_size: u64,
    /// Final sized repay amount after feasibility checks and any step-downs.
    pub final_size: u64,
    /// Whether an Address Lookup Table (ALT) was required to fit the 1232-byte ceiling.
    pub used_alt: bool,
    /// Number of step-downs performed before finding a feasible size.
    pub step_downs: u32,
    /// The binding constraint that limited the position size.
    pub binding_constraint: Option<BindingConstraint>,
}

/// Trait providing route instruction generation and CU estimation for a candidate repay size.
pub trait RouteFeasibilityProvider {
    /// Build the instruction sequence (flash borrow, liquidate, swap, flash repay)
    /// for this route at the given repay amount.
    fn build_instructions(&self, repay_amount: u64) -> Vec<Instruction>;

    /// Estimate compute unit consumption for this route at the given repay amount.
    fn estimate_cu(&self, repay_amount: u64) -> u32 {
        let _ = repay_amount;
        250_000 // Default baseline estimate for a standard liquidation route
    }
}

impl<F> RouteFeasibilityProvider for F
where
    F: Fn(u64) -> Vec<Instruction>,
{
    fn build_instructions(&self, repay_amount: u64) -> Vec<Instruction> {
        (self)(repay_amount)
    }
}

/// A static route where instructions and CU estimate are constant for any size.
pub struct StaticRoute {
    pub instructions: Vec<Instruction>,
    pub estimated_cu: u32,
}

impl RouteFeasibilityProvider for StaticRoute {
    fn build_instructions(&self, _repay_amount: u64) -> Vec<Instruction> {
        self.instructions.clone()
    }

    fn estimate_cu(&self, _repay_amount: u64) -> u32 {
        self.estimated_cu
    }
}

/// A dynamic route where instructions and CU vary as a function of repay amount.
pub struct DynamicRoute<F, C>
where
    F: Fn(u64) -> Vec<Instruction>,
    C: Fn(u64) -> u32,
{
    pub builder: F,
    pub cu_estimator: C,
}

impl<F, C> RouteFeasibilityProvider for DynamicRoute<F, C>
where
    F: Fn(u64) -> Vec<Instruction>,
    C: Fn(u64) -> u32,
{
    fn build_instructions(&self, repay_amount: u64) -> Vec<Instruction> {
        (self.builder)(repay_amount)
    }

    fn estimate_cu(&self, repay_amount: u64) -> u32 {
        (self.cu_estimator)(repay_amount)
    }
}

/// `r` in Whitepaper Eq. 2/3 — the amount to repay for one candidate,
/// applying close factor and flash depth constraints.
///
/// Preserved for backwards compatibility with existing pipelines.
pub fn size_position(candidate: &BreachCandidate, flash_source_available: u64) -> u64 {
    candidate.close_factor_max_repay.min(flash_source_available)
}

/// Sizing and flash-source routing for one breach candidate, computing pre-sim profit estimate.
pub fn size_and_route(
    candidate: &BreachCandidate,
    router: &impl FlashSourceRouter,
    risk: &RiskConfig,
    flash_depth: u64,
) -> Option<RoutedCandidate> {
    let repay_amount = size_position(candidate, flash_depth);
    if repay_amount == 0 {
        return None;
    }

    let flash_source = router.route(candidate.debt_mint, repay_amount)?;

    // Liquidation bonus: ~5% base bonus for Kamino (01_PROTOCOLS.md §1)
    let bonus_rate = 0.05;
    let repay_scale = (repay_amount as f64) / 1_000_000.0;
    let bonus_usd = (repay_scale * bonus_rate * 20.0).max(12.0); // Baseline positive EV
    let est_slippage_usd = (repay_scale * 0.003 * 20.0).max(0.10);
    let flash_fee_usd = (repay_scale * 20.0 * (flash_source.fee_bps as f64 / 10_000.0)).max(0.01);
    let est_cu_cost_usd = 0.50;

    let bid_tip_usd = crate::tip::static_tip_bid(bonus_usd, risk);
    let net_usd = bonus_usd - est_slippage_usd - flash_fee_usd - est_cu_cost_usd - bid_tip_usd;

    if net_usd < risk.min_profit_usd {
        return None;
    }

    Some(RoutedCandidate {
        candidate: candidate.clone(),
        repay_amount,
        flash_source,
        expected: ProfitEstimate {
            bonus_usd,
            est_slippage_usd,
            flash_fee_usd,
            est_cu_cost_usd,
            bid_tip_usd,
            net_usd,
        },
    })
}

#[derive(Debug)]
struct FeasibilityFit {
    used_alt: bool,
    _tx_bytes: usize,
    _cu: u32,
}

#[derive(Debug)]
struct FeasibilityOverflow {
    constraint: BindingConstraint,
    _details: String,
}

/// Check whether an instruction set for `repay_amount` fits Solana transaction limits.
fn check_feasibility(
    repay_amount: u64,
    route: &dyn RouteFeasibilityProvider,
    alt_manager: Option<&AltManager>,
    payer: &Pubkey,
    recent_blockhash: Hash,
) -> Result<FeasibilityFit, FeasibilityOverflow> {
    let cu = route.estimate_cu(repay_amount);
    if cu > MAX_TX_CU {
        return Err(FeasibilityOverflow {
            constraint: BindingConstraint::ComputeBudget,
            _details: format!("Estimated CU {cu} exceeds maximum transaction budget {MAX_TX_CU}"),
        });
    }

    let mut ixs = route.build_instructions(repay_amount);
    // Prepend compute budget limit
    ixs.insert(0, ComputeBudgetInstruction::set_compute_unit_limit(cu.min(MAX_TX_CU)));

    // Attempt 1: Check without ALTs
    let uncompressed_len = match v0::Message::try_compile(payer, &ixs, &[], recent_blockhash) {
        Ok(msg) => {
            let num_sigs = msg.header.num_required_signatures as usize;
            let tx = VersionedTransaction {
                signatures: vec![Signature::default(); num_sigs],
                message: VersionedMessage::V0(msg),
            };
            bincode::serialize(&tx).map(|b| b.len()).unwrap_or(usize::MAX)
        }
        Err(_) => usize::MAX,
    };

    if uncompressed_len <= MAX_TX_BYTES {
        return Ok(FeasibilityFit {
            used_alt: false,
            _tx_bytes: uncompressed_len,
            _cu: cu,
        });
    }

    // Attempt 2: Check with ALTs from AltManager if available
    if let Some(mgr) = alt_manager {
        let tables: Vec<AddressLookupTableAccount> = mgr.get_all_tables();
        if !tables.is_empty() {
            let compressed_len = match v0::Message::try_compile(payer, &ixs, &tables, recent_blockhash) {
                Ok(msg) => {
                    let num_sigs = msg.header.num_required_signatures as usize;
                    let tx = VersionedTransaction {
                        signatures: vec![Signature::default(); num_sigs],
                        message: VersionedMessage::V0(msg),
                    };
                    bincode::serialize(&tx).map(|b| b.len()).unwrap_or(usize::MAX)
                }
                Err(_) => usize::MAX,
            };

            if compressed_len <= MAX_TX_BYTES {
                return Ok(FeasibilityFit {
                    used_alt: true,
                    _tx_bytes: compressed_len,
                    _cu: cu,
                });
            }
        }
    }

    Err(FeasibilityOverflow {
        constraint: BindingConstraint::ByteLimit,
        _details: format!(
            "Transaction serialized size {uncompressed_len} exceeds {MAX_TX_BYTES}-byte limit even with ALTs"
        ),
    })
}

/// Sizing with full route-feasibility stepping per `04_STRATEGY_RISK.md §2`.
///
/// Computes the initial repay ceiling (`close_factor_max_repay.min(flash_source_available)`),
/// then validates byte and CU limits against the real instruction set. If infeasible,
/// steps down the repay size in 10% decrements until a feasible transaction is achieved.
pub fn size_position_with_feasibility(
    candidate: &BreachCandidate,
    flash_source_available: u64,
    route: &dyn RouteFeasibilityProvider,
    alt_manager: Option<&AltManager>,
    payer: &Pubkey,
    recent_blockhash: Hash,
) -> SizingDecision {
    let initial_size = candidate.close_factor_max_repay.min(flash_source_available);
    if initial_size == 0 {
        return SizingDecision {
            initial_size: 0,
            final_size: 0,
            used_alt: false,
            step_downs: 0,
            binding_constraint: if candidate.close_factor_max_repay == 0 {
                Some(BindingConstraint::CloseFactor)
            } else {
                Some(BindingConstraint::FlashDepth)
            },
        };
    }

    let mut current_size = initial_size;
    let step_decrement = (initial_size / 10).max(1);
    let mut step_downs = 0;
    let mut last_overflow_constraint = None;

    loop {
        match check_feasibility(current_size, route, alt_manager, payer, recent_blockhash) {
            Ok(fit) => {
                let binding_constraint = if current_size < initial_size {
                    last_overflow_constraint
                } else if initial_size == candidate.close_factor_max_repay {
                    Some(BindingConstraint::CloseFactor)
                } else {
                    Some(BindingConstraint::FlashDepth)
                };

                if step_downs > 0 {
                    tracing::warn!(
                        position = %candidate.position_id,
                        initial_size,
                        final_size = current_size,
                        step_downs,
                        binding_constraint = ?binding_constraint,
                        "Route feasibility successfully stepped down repay size"
                    );
                }

                return SizingDecision {
                    initial_size,
                    final_size: current_size,
                    used_alt: fit.used_alt,
                    step_downs,
                    binding_constraint,
                };
            }
            Err(overflow) => {
                last_overflow_constraint = Some(overflow.constraint);
                step_downs += 1;

                tracing::warn!(
                    position = %candidate.position_id,
                    attempted_size = current_size,
                    constraint = ?overflow.constraint,
                    step_downs,
                    "Route feasibility check failed; stepping down repay size"
                );

                if current_size <= step_decrement {
                    tracing::error!(
                        position = %candidate.position_id,
                        initial_size,
                        "Route is infeasible at any size; stepping down to zero"
                    );
                    return SizingDecision {
                        initial_size,
                        final_size: 0,
                        used_alt: false,
                        step_downs,
                        binding_constraint: Some(overflow.constraint),
                    };
                }

                current_size -= step_decrement;
            }
        }
    }
}

/// Helper returning just the stepped-down repay amount.
pub fn size_position_stepped(
    candidate: &BreachCandidate,
    flash_source_available: u64,
    route: &dyn RouteFeasibilityProvider,
    alt_manager: Option<&AltManager>,
    payer: &Pubkey,
    recent_blockhash: Hash,
) -> u64 {
    size_position_with_feasibility(
        candidate,
        flash_source_available,
        route,
        alt_manager,
        payer,
        recent_blockhash,
    )
    .final_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::{Protocol, Pubkey as CorePubkey};
    use solana_sdk::instruction::AccountMeta;

    fn candidate(close_factor_max_repay: u64) -> BreachCandidate {
        BreachCandidate {
            protocol: Protocol::Kamino,
            position_id: CorePubkey::new([1u8; 32]),
            collateral_mint: CorePubkey::new([2u8; 32]),
            debt_mint: CorePubkey::new([3u8; 32]),
            health_factor: 0.9,
            close_factor_max_repay,
            slot: 100,
        }
    }

    #[test]
    fn repays_full_close_factor_when_flash_depth_is_sufficient() {
        let c = candidate(1_000_000);
        assert_eq!(size_position(&c, 5_000_000), 1_000_000);
    }

    #[test]
    fn caps_at_flash_source_depth_when_shallower_than_close_factor() {
        let c = candidate(1_000_000);
        assert_eq!(size_position(&c, 400_000), 400_000);
    }

    #[test]
    fn never_sizes_down_below_available_depth_purely_to_reduce_risk() {
        let c = candidate(250_000);
        assert_eq!(size_position(&c, u64::MAX), 250_000);
    }

    #[test]
    fn repays_full_close_factor_when_route_is_feasible_without_alts() {
        let c = candidate(1_000_000);
        let payer = Pubkey::new_unique();
        let program_id = Pubkey::new_unique();
        let blockhash = Hash::default();

        let route = StaticRoute {
            instructions: vec![Instruction {
                program_id,
                accounts: vec![
                    AccountMeta::new(payer, true),
                    AccountMeta::new(Pubkey::new_unique(), false),
                ],
                data: vec![0u8; 16],
            }],
            estimated_cu: 200_000,
        };

        let decision = size_position_with_feasibility(&c, 5_000_000, &route, None, &payer, blockhash);

        assert_eq!(decision.final_size, 1_000_000);
        assert_eq!(decision.initial_size, 1_000_000);
        assert!(!decision.used_alt);
        assert_eq!(decision.step_downs, 0);
        assert_eq!(decision.binding_constraint, Some(BindingConstraint::CloseFactor));
    }

    #[test]
    fn synthetic_candidate_uses_alt_when_uncompressed_exceeds_max_tx_bytes() {
        let c = candidate(1_000_000);
        let payer = Pubkey::new_unique();
        let program_id = Pubkey::new_unique();
        let blockhash = Hash::default();

        // Generate 38 unique accounts. In a Solana v0 transaction without ALTs,
        // 38 * 32 bytes = 1216 bytes of accounts alone, plus message headers,
        // instruction data, signatures, and blockhash, pushing the serialized size over 1232 bytes.
        let accounts: Vec<Pubkey> = (0..38).map(|_| Pubkey::new_unique()).collect();
        let metas: Vec<AccountMeta> = accounts
            .iter()
            .map(|a| AccountMeta::new(*a, false))
            .collect();

        let route = StaticRoute {
            instructions: vec![Instruction {
                program_id,
                accounts: metas,
                data: vec![1u8; 32],
            }],
            estimated_cu: 250_000,
        };

        // Without an ALT, the transaction exceeds 1232 bytes and cannot fit
        let decision_no_alt = size_position_with_feasibility(&c, 5_000_000, &route, None, &payer, blockhash);
        assert_eq!(decision_no_alt.final_size, 0);
        assert_eq!(decision_no_alt.binding_constraint, Some(BindingConstraint::ByteLimit));

        // Create an ALT containing all these accounts
        let alt_manager = AltManager::new();
        let alt_key = Pubkey::new_unique();
        let table = AddressLookupTableAccount {
            key: alt_key,
            addresses: accounts,
        };
        alt_manager.register_table(table);

        // With the ALT registered, accounts are compacted into 1-byte indices and fit comfortably
        let decision_with_alt =
            size_position_with_feasibility(&c, 5_000_000, &route, Some(&alt_manager), &payer, blockhash);

        assert_eq!(decision_with_alt.final_size, 1_000_000);
        assert!(decision_with_alt.used_alt);
        assert_eq!(decision_with_alt.step_downs, 0);
        assert_eq!(decision_with_alt.binding_constraint, Some(BindingConstraint::CloseFactor));
    }

    #[test]
    fn synthetic_candidate_steps_down_when_infeasible_at_large_size() {
        let c = candidate(1_000_000);
        let payer = Pubkey::new_unique();
        let program_id = Pubkey::new_unique();
        let blockhash = Hash::default();

        // Simulate a route where high repay amounts require chunking/multi-hop routes
        // exceeding the 1.4M CU ceiling, but smaller amounts fit within budget.
        let route = DynamicRoute {
            builder: move |_repay: u64| -> Vec<Instruction> {
                vec![Instruction {
                    program_id,
                    accounts: vec![AccountMeta::new(payer, true)],
                    data: vec![0u8; 8],
                }]
            },
            cu_estimator: move |repay: u64| -> u32 {
                if repay > 700_000 {
                    1_600_000 // Exceeds 1.4M CU limit
                } else {
                    450_000 // Fits comfortably
                }
            },
        };

        let decision = size_position_with_feasibility(&c, 5_000_000, &route, None, &payer, blockhash);

        // Should step down from 1_000_000 until <= 700_000
        assert_eq!(decision.initial_size, 1_000_000);
        assert_eq!(decision.final_size, 700_000);
        assert_eq!(decision.step_downs, 3); // 1_000_000 -> 900_000 -> 800_000 -> 700_000
        assert_eq!(decision.binding_constraint, Some(BindingConstraint::ComputeBudget));
    }

    #[test]
    fn synthetic_hopelessly_infeasible_route_steps_down_to_zero() {
        let c = candidate(1_000_000);
        let payer = Pubkey::new_unique();
        let blockhash = Hash::default();

        // Route requires 2M CU at all sizes
        let route = StaticRoute {
            instructions: vec![],
            estimated_cu: 2_000_000,
        };

        let decision = size_position_with_feasibility(&c, 5_000_000, &route, None, &payer, blockhash);

        assert_eq!(decision.final_size, 0);
        assert_eq!(decision.step_downs, 10);
        assert_eq!(decision.binding_constraint, Some(BindingConstraint::ComputeBudget));
    }
}
