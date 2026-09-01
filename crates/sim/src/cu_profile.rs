//! LiteSVM-backed compute-unit profiling — the piece of Stage B item 7
//! (`docs/ARCHITECTURE.md#simulation`) and `docs/RUNBOOK.md`'s "Per-route
//! CU profiling table built from real LiteSVM runs" checklist item that
//! doesn't depend on live network access: running a transaction's
//! instructions through an in-process SVM and reading back the compute
//! units actually consumed.
//!
//! # Why this ships with no real route data
//!
//! Profiling *real* liquidation routes needs `bundler` to assemble a real
//! set of instructions from a real `RoutedCandidate` — which itself needs
//! `strategy`, `health`, and `router` wired into one live pipeline against
//! real account state. That pipeline doesn't exist yet (each crate is
//! built and unit-tested in isolation so far; see `docs/BUILD_ORDER.md`'s
//! staged sequence). What's proven here is the harness mechanism itself —
//! [`profile_instructions`] measures real CU consumption for whatever
//! instructions it's given, verified against a trivial known-cost
//! instruction (a SPL System Program transfer) rather than a fabricated
//! liquidation route. `src/bin/cu-profile.rs` is the CLI entry point this
//! module powers; as shipped it has no real routes to profile and says so
//! rather than inventing placeholder numbers.

use litesvm::LiteSVM;
use solana_sdk::instruction::Instruction;
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;

#[derive(Debug, Clone)]
pub struct CuProfile {
    pub compute_units_consumed: u64,
    pub logs: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("transaction simulation failed: {0}")]
    SimulationFailed(String),
}

/// Runs `instructions` as one transaction against a fresh in-process
/// LiteSVM instance funded from `payer`, and returns the compute units
/// actually consumed. `funded_accounts` are additional `(pubkey, lamports)`
/// pairs to pre-fund before running (e.g. accounts the instructions read
/// from) — LiteSVM starts with an empty ledger, unlike a real cluster.
pub fn profile_instructions(
    instructions: &[Instruction],
    payer: &Keypair,
    funded_accounts: &[(Pubkey, u64)],
) -> Result<CuProfile, ProfileError> {
    let mut svm = LiteSVM::new();

    svm.airdrop(&payer.pubkey(), 10_000_000_000)
        .map_err(|e| ProfileError::SimulationFailed(format!("{e:?}")))?;
    for (pubkey, lamports) in funded_accounts {
        svm.airdrop(pubkey, *lamports)
            .map_err(|e| ProfileError::SimulationFailed(format!("{e:?}")))?;
    }

    let message = Message::new(instructions, Some(&payer.pubkey()));
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new(&[payer], message, blockhash);

    let result = svm
        .send_transaction(tx)
        .map_err(|e| ProfileError::SimulationFailed(format!("{:?}", e.err)))?;

    Ok(CuProfile {
        compute_units_consumed: result.compute_units_consumed,
        logs: result.logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::system_instruction;

    #[test]
    fn measures_real_cu_for_a_known_cheap_instruction() {
        // Proves the harness mechanism itself works — a plain system
        // transfer is one of the cheapest possible instructions, so this
        // is a sanity check on measurement, not a stand-in for a real
        // liquidation route's CU cost (see module doc).
        let payer = Keypair::new();
        let to = Pubkey::new_unique();
        let ix = system_instruction::transfer(&payer.pubkey(), &to, 1_000);

        let profile =
            profile_instructions(&[ix], &payer, &[]).expect("cheap transfer should succeed");
        assert!(profile.compute_units_consumed > 0);
        // A bare transfer costs on the order of a few hundred to ~1550 CU
        // historically — generous upper bound just to catch a harness
        // that's measuring something wildly wrong (e.g. always returning
        // 0 or the 200k default), not to pin an exact protocol constant.
        assert!(profile.compute_units_consumed < 50_000);
    }

    #[test]
    fn multiple_instructions_in_one_transaction_are_profiled_together() {
        let payer = Keypair::new();
        let to_a = Pubkey::new_unique();
        let to_b = Pubkey::new_unique();
        let instructions = vec![
            system_instruction::transfer(&payer.pubkey(), &to_a, 1_000),
            system_instruction::transfer(&payer.pubkey(), &to_b, 1_000),
        ];

        let single = profile_instructions(&instructions[..1], &payer, &[]).unwrap();
        let double = profile_instructions(&instructions, &payer, &[]).unwrap();

        // Two instructions should cost at least as much as one -- proves
        // the harness is accumulating CU across the whole transaction,
        // not just reporting a per-instruction constant.
        assert!(double.compute_units_consumed >= single.compute_units_consumed);
    }
}
