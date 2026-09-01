//! Bundle builder — v0 transaction assembly, ALT management, and CU
//! budgeting, per `docs/ARCHITECTURE.md#bundle-builder` and its hard
//! constraints table (1232-byte tx size, real profiled CU limits, never
//! the runtime's 200k default).
//!
//! # Scope note
//!
//! `docs/ARCHITECTURE.md`: "`setComputeUnitLimit` set tightly per route
//! from real profiling data" — that data comes from the Stage 7 CU table
//! (`cargo run --bin cu-profile` against LiteSVM, `docs/TESTING.md`),
//! which doesn't exist until real liquidation instructions exist to
//! profile. This module accepts a measured CU value as an input
//! ([`BundleParams::cu_limit`]) rather than guessing or hardcoding one —
//! per `ARCHITECTURE.md`, a default/guessed limit is explicitly the wrong
//! answer, so there is no fallback constant here on purpose.
//!
//! Depends on `solana-sdk` directly (unlike `core`, which deliberately
//! avoids it — see `crates/core/src/pubkey.rs`) because this is exactly
//! the crate `core`'s note said would need it: real v0 transaction
//! assembly. Same MSRV caveat as `health`/`router`/`sim`: this crate could
//! not be built/tested in the sandbox that wrote it (cargo 1.75 vs.
//! solana-sdk's considerably newer MSRV).

pub mod alt;
pub mod alt_manager;
pub mod ata;
pub mod liquidations;
pub mod swaps;

pub use alt::{
    build_create_instruction, build_extend_instructions, AltError, ADDRESSES_PER_EXTEND_INSTRUCTION,
};
pub use alt_manager::AltManager;
pub use ata::{
    calculate_token_2022_transfer_fee, create_all_atas_idempotent,
    create_ata_idempotent_instruction, required_atas, spl_token_2022_program_id,
    spl_token_program_id,
};
pub use liquidations::*;
pub use swaps::*;

use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::Instruction;
use solana_sdk::message::{v0, VersionedMessage};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::VersionedTransaction;

/// Solana's own 1232-byte ceiling for a serialized transaction —
/// `ARCHITECTURE.md`'s hard constraints table. Anything over this cannot
/// be submitted at all, v0 + ALTs or not.
pub const MAX_TX_BYTES: usize = 1232;

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("failed to compile v0 message: {0}")]
    CompileMessage(#[from] solana_sdk::message::CompileError),
    #[error("failed to sign assembled transaction: {0}")]
    Sign(#[from] solana_sdk::signer::SignerError),
    #[error("failed to serialize transaction: {0}")]
    Serialize(#[from] bincode::Error),
    #[error(
        "assembled transaction is {actual} bytes, over the {MAX_TX_BYTES}-byte ceiling \
         (docs/ARCHITECTURE.md's hard constraints table) — route needs fewer accounts, \
         deeper ALT coverage, or a smaller instruction set"
    )]
    ExceedsMaxSize { actual: usize },
}

pub struct BundleParams {
    /// Every instruction for this liquidation, in execution order —
    /// flash-borrow, liquidate, swap, flash-repay, per
    /// `docs/ARCHITECTURE.md`'s flow. `setComputeUnitLimit` and (if used)
    /// `setComputeUnitPrice` are prepended by this module, not included
    /// here.
    pub instructions: Vec<Instruction>,
    /// Real, measured CU consumption for this exact route from the Stage 7
    /// CU table — never a default or a guess. See the module doc.
    pub cu_limit: u32,
    /// Optional priority fee in micro-lamports per CU, separate from any
    /// Jito tip (`ARCHITECTURE.md`).
    pub cu_price_micro_lamports: Option<u64>,
    pub payer: Pubkey,
    pub recent_blockhash: Hash,
    pub address_lookup_tables: Vec<AddressLookupTableAccount>,
}

/// Assemble one v0 transaction: prepend the compute-budget instructions,
/// compile against the given ALTs, and serialize. Returns
/// [`BundleError::ExceedsMaxSize`] rather than truncating or silently
/// submitting an oversized transaction if the 1232-byte ceiling is
/// exceeded — per `ARCHITECTURE.md`, that's a routing failure to surface,
/// not something to paper over here.
pub fn assemble<S: Signer>(
    params: BundleParams,
    signer: &S,
) -> Result<VersionedTransaction, BundleError> {
    let mut instructions = Vec::with_capacity(params.instructions.len() + 2);
    instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
        params.cu_limit,
    ));
    if let Some(price) = params.cu_price_micro_lamports {
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(price));
    }
    instructions.extend(params.instructions);

    let message = v0::Message::try_compile(
        &params.payer,
        &instructions,
        &params.address_lookup_tables,
        params.recent_blockhash,
    )?;
    let versioned_message = VersionedMessage::V0(message);

    let tx = VersionedTransaction::try_new(versioned_message, &[signer])?;

    let bytes = bincode::serialize(&tx)?;
    if bytes.len() > MAX_TX_BYTES {
        return Err(BundleError::ExceedsMaxSize {
            actual: bytes.len(),
        });
    }

    Ok(tx)
}

/// Convert an assembled transaction into the opaque-bytes form
/// `gyrfalcon_core::Bundle::versioned_tx` expects — see
/// `crates/core/src/pubkey.rs`'s note on why `core` stores this as
/// `Vec<u8>` rather than depending on `solana-sdk` itself.
pub fn to_core_bundle_bytes(tx: &VersionedTransaction) -> Result<Vec<u8>, BundleError> {
    Ok(bincode::serialize(tx)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Keypair;
    use solana_sdk::system_instruction;

    #[test]
    fn assembles_a_small_transaction_under_the_size_ceiling() {
        let payer = Keypair::new();
        let to = Pubkey::new_unique();
        let ix = system_instruction::transfer(&payer.pubkey(), &to, 1_000);

        let params = BundleParams {
            instructions: vec![ix],
            cu_limit: 5_000,
            cu_price_micro_lamports: Some(1_000),
            payer: payer.pubkey(),
            recent_blockhash: Hash::default(),
            address_lookup_tables: vec![],
        };

        let tx = assemble(params, &payer).expect("small tx should assemble under the ceiling");
        let bytes = to_core_bundle_bytes(&tx).unwrap();
        assert!(bytes.len() <= MAX_TX_BYTES);
    }

    #[test]
    fn oversized_instruction_set_is_rejected_not_truncated() {
        let payer = Keypair::new();
        // Enough transfer instructions (each ~a few dozen bytes once
        // compiled) to blow past 1232 bytes without any ALT coverage.
        let instructions: Vec<Instruction> = (0..40)
            .map(|_| system_instruction::transfer(&payer.pubkey(), &Pubkey::new_unique(), 1))
            .collect();

        let params = BundleParams {
            instructions,
            cu_limit: 200_000,
            cu_price_micro_lamports: None,
            payer: payer.pubkey(),
            recent_blockhash: Hash::default(),
            address_lookup_tables: vec![],
        };

        let result = assemble(params, &payer);
        match result {
            Err(BundleError::ExceedsMaxSize { actual }) => assert!(actual > MAX_TX_BYTES),
            other => panic!("expected ExceedsMaxSize, got {other:?}"),
        }
    }
}
