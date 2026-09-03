//! Address Lookup Table creation and extension — `docs/RUNBOOK.md`'s
//! "ALTs built and warmed for every route intended to fire on day one."
//!
//! This module covers the "built" half: constructing the
//! `create_lookup_table` / `extend_lookup_table` instructions is a pure,
//! deterministic operation given a `recent_slot` and address list — no
//! live network needed to get the instructions themselves right. The
//! "warmed" half is fundamentally not something this module can do:
//! per Solana's own ALT design, a freshly-extended table only becomes
//! usable in transactions once the slot it was extended in is no longer
//! the most recent slot (roughly: one slot of confirmation lag) — that's
//! a live-chain waiting condition, not a computation. A caller wires this
//! module's instructions into real `submit`/RPC calls and then waits for
//! that confirmation before treating a route as fire-ready; this module
//! stops at "here are the instructions to send."

use solana_sdk::address_lookup_table::instruction::{create_lookup_table, extend_lookup_table};
use solana_sdk::address_lookup_table::state::LOOKUP_TABLE_MAX_ADDRESSES;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;

/// Solana enforces up to `LOOKUP_TABLE_MAX_ADDRESSES` (256) total entries
/// per table, but a single `extend_lookup_table` instruction is also
/// bounded by the 1232-byte transaction ceiling once wrapped in a real
/// transaction alongside its own overhead. 20 addresses/instruction is a
/// conservative batch size that leaves headroom for that overhead without
/// needing this module to reason about a specific transaction's other
/// contents.
pub const ADDRESSES_PER_EXTEND_INSTRUCTION: usize = 20;

#[derive(Debug, thiserror::Error)]
pub enum AltError {
    #[error(
        "{0} addresses requested, over the {LOOKUP_TABLE_MAX_ADDRESSES}-per-table ceiling \
         Solana enforces — split across multiple lookup tables"
    )]
    ExceedsTableCapacity(usize),
}

/// Builds the `create_lookup_table` instruction and returns the ALT's
/// derived address alongside it (the caller needs the address to build
/// the following `extend` instructions and, later, to reference the ALT
/// in `bundler::BundleParams::address_lookup_tables`).
///
/// `recent_slot` must be a slot the cluster has already reached — passing
/// a future or too-recent slot is rejected on-chain, per Solana's ALT
/// program rules; this module doesn't validate that itself since it has
/// no notion of "current slot" without a live connection.
pub fn build_create_instruction(
    authority: &Pubkey,
    payer: &Pubkey,
    recent_slot: u64,
) -> (Pubkey, Instruction) {
    let (ix, pubkey) = create_lookup_table(*authority, *payer, recent_slot);
    (pubkey, ix)
}

/// Splits `addresses` into as many `extend_lookup_table` instructions as
/// needed, respecting both the per-instruction batch size and the
/// table-wide capacity.
pub fn build_extend_instructions(
    alt_address: &Pubkey,
    authority: &Pubkey,
    payer: &Pubkey,
    addresses: &[Pubkey],
) -> Result<Vec<Instruction>, AltError> {
    if addresses.len() > LOOKUP_TABLE_MAX_ADDRESSES {
        return Err(AltError::ExceedsTableCapacity(addresses.len()));
    }

    Ok(addresses
        .chunks(ADDRESSES_PER_EXTEND_INSTRUCTION)
        .map(|chunk| extend_lookup_table(*alt_address, *authority, Some(*payer), chunk.to_vec()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_instruction_derives_a_deterministic_alt_address() {
        let authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (alt_a, _ix_a) = build_create_instruction(&authority, &payer, 1_000);
        let (alt_b, _ix_b) = build_create_instruction(&authority, &payer, 1_000);
        // Same inputs -> same derived address (it's a PDA), proving this
        // is deterministic rather than randomly generated.
        assert_eq!(alt_a, alt_b);
    }

    #[test]
    fn different_recent_slot_derives_a_different_alt_address() {
        let authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (alt_a, _) = build_create_instruction(&authority, &payer, 1_000);
        let (alt_b, _) = build_create_instruction(&authority, &payer, 2_000);
        assert_ne!(alt_a, alt_b);
    }

    #[test]
    fn splits_addresses_into_batches_of_the_configured_size() {
        let alt = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let addresses: Vec<Pubkey> = (0..45).map(|_| Pubkey::new_unique()).collect();

        let instructions = build_extend_instructions(&alt, &authority, &payer, &addresses).unwrap();
        // 45 addresses / 20 per instruction -> 3 instructions (20, 20, 5).
        assert_eq!(instructions.len(), 3);
    }

    #[test]
    fn rejects_more_addresses_than_a_single_table_can_hold() {
        let alt = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let addresses: Vec<Pubkey> = (0..300).map(|_| Pubkey::new_unique()).collect();

        let result = build_extend_instructions(&alt, &authority, &payer, &addresses);
        assert!(matches!(result, Err(AltError::ExceedsTableCapacity(300))));
    }
}
