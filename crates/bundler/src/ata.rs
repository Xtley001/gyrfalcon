//! ATA pre-provisioning — `docs/RUNBOOK.md`'s production readiness
//! checklist item "ATA pre-provisioning complete for every mint
//! combination across all three protocols."
//!
//! Deriving an Associated Token Account's address is a pure, deterministic
//! PDA computation (owner + token program + mint, against the fixed
//! `spl-associated-token-account` program) — it needs no live RPC access
//! to get right, unlike checking whether that address is *already
//! created* on-chain (which does need RPC, and is explicitly out of scope
//! here — see [`AtaError`] doc and this module's `check_existence` note).
//! Uses the official `spl-associated-token-account` crate rather than
//! hand-deriving the PDA, same principle as every other protocol
//! dependency in this codebase (`kamino.rs`, `save.rs`, `marginfi.rs`).

use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;

/// The classic SPL Token program's well-known mainnet address.
pub fn spl_token_program_id() -> Pubkey {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .expect("hardcoded SPL Token program ID must be valid")
}

/// The SPL Token-2022 (Extensions) program's well-known mainnet address.
pub fn spl_token_2022_program_id() -> Pubkey {
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        .parse()
        .expect("hardcoded SPL Token-2022 program ID must be valid")
}

/// Calculate transfer fee for Token-2022 with transfer-fee extension enabled.
/// Returns fee in raw token base units.
pub fn calculate_token_2022_transfer_fee(
    pre_fee_amount: u64,
    fee_basis_points: u16,
    max_fee: u64,
) -> u64 {
    let numerator = (pre_fee_amount as u128) * (fee_basis_points as u128);
    let fee = (numerator / 10_000) as u64;
    fee.min(max_fee)
}

/// Every ATA the engine's own signer needs to hold liquidated collateral
/// and repay flash loans for one route, deduplicated. `mints` should be
/// the full set of collateral/debt mints across every reserve the engine
/// watches, not just one route — this is meant to be run once up front
/// (per the checklist item's "for every mint combination"), not
/// recomputed per candidate.
pub fn required_atas(owner: &Pubkey, mints: &[Pubkey], token_program: &Pubkey) -> Vec<Pubkey> {
    let mut seen = std::collections::HashSet::new();
    mints
        .iter()
        .filter_map(|mint| {
            let ata = spl_associated_token_account::get_associated_token_address_with_program_id(
                owner,
                mint,
                token_program,
            );
            seen.insert(ata).then_some(ata)
        })
        .collect()
}

/// Idempotent create instruction for one ATA — safe to include even if the
/// account might already exist (the instruction is a no-op on-chain in
/// that case rather than erroring), which is exactly the right primitive
/// for a pre-provisioning pass that doesn't first check existence via RPC.
pub fn create_ata_idempotent_instruction(
    payer: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> Instruction {
    spl_associated_token_account::instruction::create_associated_token_account_idempotent(
        payer,
        owner,
        mint,
        token_program,
    )
}

/// Builds one idempotent-create instruction per mint, deduplicated. A
/// caller can batch these into as many transactions as the 1232-byte
/// ceiling allows (`bundler::MAX_TX_BYTES`) — batching itself isn't done
/// here since the right batch size depends on how many other instructions
/// (if any) share the transaction, which this module has no opinion on.
pub fn create_all_atas_idempotent(
    payer: &Pubkey,
    owner: &Pubkey,
    mints: &[Pubkey],
    token_program: &Pubkey,
) -> Vec<Instruction> {
    let mut seen = std::collections::HashSet::new();
    mints
        .iter()
        .filter(|mint| seen.insert(**mint))
        .map(|mint| create_ata_idempotent_instruction(payer, owner, mint, token_program))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_the_same_address_as_the_official_spl_helper() {
        let owner = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let token_program = spl_token_program_id();

        let derived = required_atas(&owner, &[mint], &token_program);
        let expected = spl_associated_token_account::get_associated_token_address_with_program_id(
            &owner,
            &mint,
            &token_program,
        );

        assert_eq!(derived, vec![expected]);
    }

    #[test]
    fn deduplicates_repeated_mints() {
        let owner = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let token_program = spl_token_program_id();

        let derived = required_atas(&owner, &[mint, mint, mint], &token_program);
        assert_eq!(derived.len(), 1);
    }

    #[test]
    fn different_mints_produce_different_atas() {
        let owner = Pubkey::new_unique();
        let mint_a = Pubkey::new_unique();
        let mint_b = Pubkey::new_unique();
        let token_program = spl_token_program_id();

        let derived = required_atas(&owner, &[mint_a, mint_b], &token_program);
        assert_eq!(derived.len(), 2);
        assert_ne!(derived[0], derived[1]);
    }

    #[test]
    fn create_all_atas_idempotent_produces_one_instruction_per_unique_mint() {
        let payer = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let mint_a = Pubkey::new_unique();
        let mint_b = Pubkey::new_unique();
        let token_program = spl_token_program_id();

        let instructions =
            create_all_atas_idempotent(&payer, &owner, &[mint_a, mint_b, mint_a], &token_program);
        assert_eq!(instructions.len(), 2);
    }
}
