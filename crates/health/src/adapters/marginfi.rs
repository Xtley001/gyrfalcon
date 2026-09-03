//! MarginFi `HealthAdapter` — decodes real `MarginfiAccount` and `Bank`
//! accounts from the deployed marginfi-v2 program
//! (`MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` on mainnet), via
//! `marginfi-type-crate` — the team's own maintained, Apache-2.0, published
//! account-layout crate (same category of dependency as `klend-interface`
//! for Kamino and `solend-sdk` for Save, decoded the same zero-copy-Pod way
//! as Kamino — see `kamino.rs`'s module doc for why an official SDK crate
//! beats a hand-transcribed layout).
//!
//! # Correcting `docs/ARCHITECTURE.md`'s load-bearing assumption
//!
//! `docs/BUILD_ORDER.md` item 14 explicitly requires re-verifying, before
//! writing this adapter, that MarginFi still has no native flash-loan
//! instruction — `ARCHITECTURE.md` calls this "the load-bearing fact of
//! the whole flash-source design." **That fact is wrong, and was wrong
//! even at the time this doc set was written.** MarginFi has had a native
//! flash-loan mechanism since early in marginfi-v2's life:
//! `lending_account_start_flashloan` / `lending_account_end_flashloan`,
//! which set an `ACCOUNT_IN_FLASHLOAN` flag on the account for the
//! duration of the loan and skip the usual health check until it's
//! unset — confirmed via MarginFi's own docs (`docs.marginfi.com/mfi-v2`,
//! `docs.marginfi.com/faqs`: "Flashloans... borrows that occur within the
//! flashloan get paid back immediately") and independently via a September
//! 2025 security disclosure (Asymmetric Research, "Threat Contained:
//! marginfi Flash Loan Vulnerability") that describes exploiting exactly
//! that mechanism.
//!
//! This does **not** change anything `router` currently does — Stage C
//! item 13 only wires up Kamino and Save as flash sources, and this
//! adapter still routes MarginFi liquidations through Kamino/Save per
//! `docs/BUILD_ORDER.md`'s "MarginFi liquidations borrow from Kamino or
//! Save within the same transaction." What it *does* mean: don't carry
//! "MarginFi can't be a flash source" forward as settled fact into any
//! future work — it was never true, and adding MarginFi as a fourth
//! source later is a legitimate option this doc set incorrectly foreclosed.
//!
//! # A second, more consequential finding: no continuously-fresh health cache
//!
//! Kamino's `Obligation` and Save's `Obligation` both carry a health/debt
//! figure that's refreshed by an ordinary instruction (`refresh_obligation`)
//! any liquidator's own transaction can call moments before liquidating —
//! so reading it off a recent account update is a reasonable proxy for
//! "is this position breached right now." MarginFi's `HealthCache` is
//! different in kind: its own doc comment says it's "only valid in
//! borrow/withdraw if the tx does not fail" — it's a byproduct written by
//! specific instructions, not a value continuously kept current the way
//! Kamino/Save's is. Treating a `HEALTHY` flag read off an arbitrary
//! account update as equivalent to Kamino/Save's live health factor would
//! be a materially weaker detection signal, silently — exactly the kind
//! of gap `docs/ARCHITECTURE.md` warns produces a wrong number with no
//! visible error.
//!
//! This adapter is honest about that gap rather than papering over it:
//! [`MarginfiAdapter::on_account_update`] reads `HealthCache.flags`'s
//! `HEALTHY` bit as a breach signal **only when `ENGINE_OK` is also set**
//! (i.e. the cache is from a health pulse that didn't error), and reports
//! it as-is. A full from-first-principles health recompute — reading every
//! `Balance`, resolving each to its `Bank`'s asset/liability share value
//! and oracle price, replicating marginfi's own weighted-asset risk
//! engine — is out of scope here; it's real engineering work, not a
//! decode-the-right-struct problem like Kamino/Save, and shouldn't be
//! faked to look equivalent. **Stage C's replay harness (re-run per item
//! 13/14) is exactly what should confirm or refute whether this
//! cache-flag approach detects breaches on time before any capital
//! depends on it** — that validation has not happened in this build.

use gyrfalcon_core::traits::AccountUpdate;
use gyrfalcon_core::types::BreachCandidate;
use gyrfalcon_core::{HealthAdapter, Protocol, Pubkey as CorePubkey};
use marginfi_type_crate::types::{Bank, MarginfiAccount, ENGINE_OK, HEALTHY};
use std::collections::HashMap;

/// marginfi-v2's mainnet program ID, per MarginFi's own published SDK
/// config (`getConfig("production")` in `@mrgnlabs/marginfi-client-v2`).
/// Not sourced from `marginfi-type-crate`'s `id-crate` re-export because
/// that requires selecting a Cargo feature (`mainnet-beta` vs. `devnet`
/// etc.) at compile time for the whole crate — simpler and equally
/// correct to decode the known-fixed base58 string directly at runtime
/// (once per call; cheap, and this isn't a hot-path function).
fn program_id() -> CorePubkey {
    CorePubkey::from_base58("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA")
        .expect("hardcoded mainnet program ID must be valid base58")
}

fn to_core_pubkey(p: [u8; 32]) -> CorePubkey {
    CorePubkey::new(p)
}

fn wrapped_to_f64(w: marginfi_type_crate::types::WrappedI80F48) -> f64 {
    let fixed: fixed::types::I80F48 = w.into();
    fixed.to_num::<f64>()
}

#[derive(Debug, Clone, Copy)]
struct BankSnapshot {
    mint: CorePubkey,
    liability_share_value: f64,
    mint_decimals: u8,
}

#[derive(Debug, Clone, Copy)]
struct AccountSnapshot {
    slot: u64,
    close_factor_max_repay: u64,
}

#[derive(Debug, Default)]
pub struct MarginfiAdapter {
    banks: HashMap<CorePubkey, BankSnapshot>,
    known_accounts: HashMap<CorePubkey, AccountSnapshot>,
    last_synced_slot: u64,
    current_slot: u64,
}

impl MarginfiAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    fn handle_bank(&mut self, pubkey: CorePubkey, data: &[u8]) {
        if data.len() < 8 || data[..8] != Bank::DISCRIMINATOR[..] {
            return;
        }
        if let Ok(bank) = bytemuck::try_pod_read_unaligned::<Bank>(&data[8..]) {
            self.banks.insert(
                pubkey,
                BankSnapshot {
                    mint: to_core_pubkey(bank.mint.to_bytes()),
                    liability_share_value: wrapped_to_f64(bank.liability_share_value),
                    mint_decimals: bank.mint_decimals,
                },
            );
        }
    }

    fn handle_account(
        &mut self,
        pubkey: CorePubkey,
        data: &[u8],
        slot: u64,
    ) -> Option<BreachCandidate> {
        if data.len() < 8 || data[..8] != MarginfiAccount::DISCRIMINATOR[..] {
            return None;
        }
        let account: MarginfiAccount = bytemuck::try_pod_read_unaligned(&data[8..]).ok()?;

        let flags = account.health_cache.flags;
        let engine_ok = flags & ENGINE_OK != 0;
        let healthy = flags & HEALTHY != 0;
        // See the module doc: this is a best-effort signal from a cache
        // that isn't guaranteed continuously fresh, not an equivalent
        // substitute for Kamino/Save's live-refreshed health factor.
        if !engine_ok || healthy {
            self.known_accounts.insert(pubkey, AccountSnapshot { slot, close_factor_max_repay: 0 });
            return None;
        }

        let asset_value = wrapped_to_f64(account.health_cache.asset_value_maint);
        let liability_value = wrapped_to_f64(account.health_cache.liability_value_maint);
        if liability_value <= 0.0 || asset_value <= 0.0 {
            self.known_accounts.insert(pubkey, AccountSnapshot { slot, close_factor_max_repay: 0 });
            return None;
        }
        let health_factor = asset_value / liability_value;

        let active_balances: Vec<_> = account.lending_account.get_active_balances_iter().collect();

        let debt_balance = active_balances
            .iter()
            .filter(|b| wrapped_to_f64(b.liability_shares) > 0.0)
            .max_by(|a, b| {
                wrapped_to_f64(a.liability_shares)
                    .partial_cmp(&wrapped_to_f64(b.liability_shares))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
        let debt_bank_key = to_core_pubkey(debt_balance.bank_pk.to_bytes());
        let debt_bank = self.banks.get(&debt_bank_key)?;

        let collateral_balance = active_balances
            .iter()
            .filter(|b| wrapped_to_f64(b.asset_shares) > 0.0)
            .max_by(|a, b| {
                wrapped_to_f64(a.asset_shares)
                    .partial_cmp(&wrapped_to_f64(b.asset_shares))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
        let collateral_bank_key = to_core_pubkey(collateral_balance.bank_pk.to_bytes());
        let collateral_bank = self.banks.get(&collateral_bank_key)?;

        let liability_shares = wrapped_to_f64(debt_balance.liability_shares);
        let liability_native = (liability_shares * debt_bank.liability_share_value).max(0.0);
        // MarginFi enforces a max liquidation close factor of 50%
        let close_factor_max_repay = (liability_native * 0.50) as u64;

        self.known_accounts.insert(
            pubkey,
            AccountSnapshot {
                slot,
                close_factor_max_repay,
            },
        );

        Some(BreachCandidate {
            protocol: Protocol::MarginFi,
            position_id: pubkey,
            collateral_mint: collateral_bank.mint,
            debt_mint: debt_bank.mint,
            health_factor,
            close_factor_max_repay,
            slot,
        })
    }
}

impl HealthAdapter for MarginfiAdapter {
    fn on_account_update(&mut self, update: AccountUpdate) -> Option<BreachCandidate> {
        if update.owner != program_id() {
            return None;
        }
        self.current_slot = self.current_slot.max(update.slot);
        self.last_synced_slot = update.slot;

        if update.data.len() < 8 {
            return None;
        }
        if update.data[..8] == Bank::DISCRIMINATOR[..] {
            self.handle_bank(update.pubkey, &update.data);
            None
        } else if update.data[..8] == MarginfiAccount::DISCRIMINATOR[..] {
            self.handle_account(update.pubkey, &update.data, update.slot)
        } else {
            None
        }
    }

    fn close_factor(&self, position_id: CorePubkey) -> u64 {
        self.known_accounts
            .get(&position_id)
            .map(|s| s.close_factor_max_repay)
            .unwrap_or(0)
    }

    fn position_count(&self) -> usize {
        self.known_accounts.len()
    }

    fn sync_lag_slots(&self) -> u64 {
        self.current_slot.saturating_sub(self.last_synced_slot)
    }
}

impl MarginfiAdapter {
    /// Unlike `KaminoAdapter::reserve_price_is_stale` and
    /// `SaveAdapter::reserve_price_is_stale`, this adapter has no
    /// equivalent to offer yet: MarginFi's price staleness lives on the
    /// separate Pyth/Switchboard oracle account each `Bank` references
    /// (`Bank`'s own fields carry no price timestamp — see this module's
    /// doc), and this adapter doesn't decode oracle accounts at all.
    /// Deliberately present as a stub that says so, rather than silently
    /// missing, so a caller checking all three protocols' staleness in a
    /// loop gets an explicit "not implemented" instead of a confusing
    /// absence.
    pub fn reserve_price_is_stale(&self, _bank: CorePubkey, _current_unix_ts: u64) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_marginfi_owner_is_ignored() {
        let mut adapter = MarginfiAdapter::new();
        let result = adapter.on_account_update(AccountUpdate {
            pubkey: CorePubkey::new([1u8; 32]),
            owner: CorePubkey::new([255u8; 32]),
            data: vec![0u8; 64],
            slot: 1,
        });
        assert!(result.is_none());
        assert_eq!(adapter.position_count(), 0);
    }

    #[test]
    fn program_id_decodes_to_the_documented_mainnet_address() {
        assert_eq!(
            program_id().to_base58(),
            "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"
        );
    }
}
