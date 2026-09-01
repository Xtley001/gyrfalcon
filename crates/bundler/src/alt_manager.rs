//! Automated Address Lookup Table (ALT) manager.
//!
//! Creates, extends, and caches ALTs for high-velocity liquidation routes
//! to guarantee transactions stay within the 1232-byte limit.

use crate::alt::{build_create_instruction, build_extend_instructions, AltError};
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Thread-safe in-memory cache of Address Lookup Tables.
#[derive(Clone, Default)]
pub struct AltManager {
    tables: Arc<RwLock<HashMap<Pubkey, AddressLookupTableAccount>>>,
}

impl AltManager {
    pub fn new() -> Self {
        Self {
            tables: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a populated AddressLookupTableAccount in cache.
    pub fn register_table(&self, table: AddressLookupTableAccount) {
        if let Ok(mut map) = self.tables.write() {
            map.insert(table.key, table);
        }
    }

    /// Retrieve an AddressLookupTableAccount by key.
    pub fn get_table(&self, key: &Pubkey) -> Option<AddressLookupTableAccount> {
        self.tables.read().ok()?.get(key).cloned()
    }

    /// Plan the creation and batch extension of a new ALT for a set of route addresses.
    /// Returns (alt_address, init_instruction, extend_instructions).
    pub fn plan_create_and_populate(
        &self,
        payer: &Pubkey,
        authority: &Pubkey,
        recent_slot: u64,
        addresses: &[Pubkey],
    ) -> Result<(Pubkey, Instruction, Vec<Instruction>), AltError> {
        let (create_ix, alt_pubkey) = build_create_instruction(payer, authority, recent_slot);
        let extend_ixs = build_extend_instructions(&alt_pubkey, payer, authority, addresses)?;

        Ok((alt_pubkey, create_ix, extend_ixs))
    }

    /// Retrieve all registered lookup tables matching a set of required table keys.
    pub fn get_lookup_tables(&self, keys: &[Pubkey]) -> Vec<AddressLookupTableAccount> {
        let guard = match self.tables.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };

        keys.iter().filter_map(|k| guard.get(k).cloned()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alt_manager_lifecycle() {
        let manager = AltManager::new();
        let payer = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let addrs: Vec<Pubkey> = (0..50).map(|_| Pubkey::new_unique()).collect();

        let (alt_key, create_ix, extend_ixs) = manager
            .plan_create_and_populate(&payer, &authority, 100, &addrs)
            .expect("should plan ALT creation successfully");

        assert_eq!(extend_ixs.len(), 2); // 50 addresses = 30 + 20 = 2 batches
        assert_eq!(create_ix.program_id, solana_sdk::address_lookup_table::program::id());

        let table_acc = AddressLookupTableAccount {
            key: alt_key,
            addresses: addrs.clone(),
        };
        manager.register_table(table_acc.clone());

        let retrieved = manager.get_table(&alt_key).expect("table should be cached");
        assert_eq!(retrieved.addresses.len(), 50);
    }
}
