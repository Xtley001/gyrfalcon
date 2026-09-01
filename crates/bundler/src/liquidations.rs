//! Protocol liquidation and flash-loan instruction builders for Kamino, Save, and MarginFi.

use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;

/// Well-known protocol program IDs on Solana Mainnet-Beta.
pub mod programs {
    use solana_sdk::pubkey::Pubkey;

    pub fn kamino_program_id() -> Pubkey {
        "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD"
            .parse()
            .unwrap()
    }

    pub fn save_program_id() -> Pubkey {
        "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo"
            .parse()
            .unwrap()
    }

    pub fn marginfi_program_id() -> Pubkey {
        "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"
            .parse()
            .unwrap()
    }
}

/// Parameters for constructing a Kamino Lend liquidation instruction.
#[derive(Debug, Clone)]
pub struct KaminoLiquidateParams {
    pub lending_market: Pubkey,
    pub obligation: Pubkey,
    pub repay_reserve: Pubkey,
    pub repay_reserve_liquidity_supply: Pubkey,
    pub withdraw_reserve: Pubkey,
    pub withdraw_reserve_collateral_supply: Pubkey,
    pub user_source_liquidity: Pubkey,
    pub user_destination_collateral: Pubkey,
    pub user_destination_liquidity: Pubkey,
    pub obligation_owner: Pubkey,
    pub liquidator: Pubkey,
    pub token_program: Pubkey,
    pub liquidity_amount: u64,
    pub min_acceptable_received_collateral: u64,
}

/// Build Kamino `LiquidateObligationAndRedeemReserveCollateral` instruction.
pub fn build_kamino_liquidate_instruction(params: KaminoLiquidateParams) -> Instruction {
    // Instruction discriminator for klend::liquidateObligationAndRedeemReserveCollateral
    let mut data = Vec::with_capacity(24);
    // Anchor discriminator: sha256("global:liquidate_obligation_and_redeem_reserve_collateral")[..8]
    data.extend_from_slice(&[0xb8, 0xb8, 0x48, 0x4f, 0x86, 0x3a, 0x8d, 0xd0]);
    data.extend_from_slice(&params.liquidity_amount.to_le_bytes());
    data.extend_from_slice(&params.min_acceptable_received_collateral.to_le_bytes());

    let accounts = vec![
        AccountMeta::new(params.liquidator, true),
        AccountMeta::new(params.obligation, false),
        AccountMeta::new_readonly(params.lending_market, false),
        AccountMeta::new(params.repay_reserve, false),
        AccountMeta::new(params.repay_reserve_liquidity_supply, false),
        AccountMeta::new(params.withdraw_reserve, false),
        AccountMeta::new(params.withdraw_reserve_collateral_supply, false),
        AccountMeta::new(params.user_source_liquidity, false),
        AccountMeta::new(params.user_destination_collateral, false),
        AccountMeta::new(params.user_destination_liquidity, false),
        AccountMeta::new_readonly(params.token_program, false),
    ];

    Instruction {
        program_id: programs::kamino_program_id(),
        accounts,
        data,
    }
}

/// Parameters for constructing a Save (Solend) liquidation instruction.
#[derive(Debug, Clone)]
pub struct SaveLiquidateParams {
    pub source_liquidity: Pubkey,
    pub destination_collateral: Pubkey,
    pub repay_reserve: Pubkey,
    pub repay_reserve_liquidity_supply: Pubkey,
    pub withdraw_reserve: Pubkey,
    pub withdraw_reserve_collateral_supply: Pubkey,
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    pub lending_market_authority: Pubkey,
    pub user_transfer_authority: Pubkey,
    pub token_program: Pubkey,
    pub liquidity_amount: u64,
}

/// Build Save / Solend `LiquidateObligationAndRedeemReserveCollateral` instruction.
pub fn build_save_liquidate_instruction(params: SaveLiquidateParams) -> Instruction {
    // Solend instruction tag: 13 = LiquidateObligationAndRedeemReserveCollateral
    let mut data = Vec::with_capacity(9);
    data.push(13u8);
    data.extend_from_slice(&params.liquidity_amount.to_le_bytes());

    let accounts = vec![
        AccountMeta::new(params.source_liquidity, false),
        AccountMeta::new(params.destination_collateral, false),
        AccountMeta::new(params.repay_reserve, false),
        AccountMeta::new(params.repay_reserve_liquidity_supply, false),
        AccountMeta::new(params.withdraw_reserve, false),
        AccountMeta::new(params.withdraw_reserve_collateral_supply, false),
        AccountMeta::new(params.obligation, false),
        AccountMeta::new_readonly(params.lending_market, false),
        AccountMeta::new_readonly(params.lending_market_authority, false),
        AccountMeta::new_readonly(params.user_transfer_authority, true),
        AccountMeta::new_readonly(params.token_program, false),
    ];

    Instruction {
        program_id: programs::save_program_id(),
        accounts,
        data,
    }
}

/// Parameters for constructing a MarginFi liquidation instruction.
#[derive(Debug, Clone)]
pub struct MarginfiLiquidateParams {
    pub marginfi_group: Pubkey,
    pub asset_bank: Pubkey,
    pub liab_bank: Pubkey,
    pub liquidator_marginfi_account: Pubkey,
    pub signer: Pubkey,
    pub liquidated_marginfi_account: Pubkey,
    pub asset_bank_liquidity_vault: Pubkey,
    pub liab_bank_liquidity_vault: Pubkey,
    pub asset_bank_liquidity_vault_authority: Pubkey,
    pub liab_bank_liquidity_vault_authority: Pubkey,
    pub asset_token_program: Pubkey,
    pub liab_token_program: Pubkey,
    pub asset_amount: u64,
}

/// Build MarginFi v2 `lending_account_liquidate` instruction.
pub fn build_marginfi_liquidate_instruction(params: MarginfiLiquidateParams) -> Instruction {
    // Anchor discriminator: sha256("global:lending_account_liquidate")[..8]
    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&[0xea, 0xef, 0x1d, 0x05, 0x16, 0x6a, 0x82, 0x88]);
    data.extend_from_slice(&params.asset_amount.to_le_bytes());

    let accounts = vec![
        AccountMeta::new_readonly(params.marginfi_group, false),
        AccountMeta::new(params.asset_bank, false),
        AccountMeta::new(params.liab_bank, false),
        AccountMeta::new(params.liquidator_marginfi_account, false),
        AccountMeta::new_readonly(params.signer, true),
        AccountMeta::new(params.liquidated_marginfi_account, false),
        AccountMeta::new(params.asset_bank_liquidity_vault, false),
        AccountMeta::new(params.liab_bank_liquidity_vault, false),
        AccountMeta::new_readonly(params.asset_bank_liquidity_vault_authority, false),
        AccountMeta::new_readonly(params.liab_bank_liquidity_vault_authority, false),
        AccountMeta::new_readonly(params.asset_token_program, false),
        AccountMeta::new_readonly(params.liab_token_program, false),
    ];

    Instruction {
        program_id: programs::marginfi_program_id(),
        accounts,
        data,
    }
}

/// Build Flash Loan Borrow instruction (Kamino / MarginFi / Save).
pub fn build_flash_borrow_instruction(
    program_id: Pubkey,
    borrow_reserve: Pubkey,
    liquidity_supply: Pubkey,
    destination_liquidity: Pubkey,
    token_program: Pubkey,
    amount: u64,
) -> Instruction {
    let mut data = Vec::with_capacity(9);
    data.push(14u8); // Generic flash borrow discriminator
    data.extend_from_slice(&amount.to_le_bytes());

    let accounts = vec![
        AccountMeta::new(borrow_reserve, false),
        AccountMeta::new(liquidity_supply, false),
        AccountMeta::new(destination_liquidity, false),
        AccountMeta::new_readonly(token_program, false),
    ];

    Instruction {
        program_id,
        accounts,
        data,
    }
}

/// Build Flash Loan Repay instruction.
pub fn build_flash_repay_instruction(
    program_id: Pubkey,
    repay_reserve: Pubkey,
    source_liquidity: Pubkey,
    liquidity_supply: Pubkey,
    user_authority: Pubkey,
    token_program: Pubkey,
    amount: u64,
    borrow_instruction_index: u8,
) -> Instruction {
    let mut data = Vec::with_capacity(10);
    data.push(15u8); // Generic flash repay discriminator
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(borrow_instruction_index);

    let accounts = vec![
        AccountMeta::new(repay_reserve, false),
        AccountMeta::new(source_liquidity, false),
        AccountMeta::new(liquidity_supply, false),
        AccountMeta::new_readonly(user_authority, true),
        AccountMeta::new_readonly(token_program, false),
    ];

    Instruction {
        program_id,
        accounts,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_kamino_liquidate_instruction() {
        let dummy = Pubkey::new_unique();
        let params = KaminoLiquidateParams {
            lending_market: dummy,
            obligation: dummy,
            repay_reserve: dummy,
            repay_reserve_liquidity_supply: dummy,
            withdraw_reserve: dummy,
            withdraw_reserve_collateral_supply: dummy,
            user_source_liquidity: dummy,
            user_destination_collateral: dummy,
            user_destination_liquidity: dummy,
            obligation_owner: dummy,
            liquidator: dummy,
            token_program: dummy,
            liquidity_amount: 1_000_000,
            min_acceptable_received_collateral: 500_000,
        };

        let ix = build_kamino_liquidate_instruction(params);
        assert_eq!(ix.program_id, programs::kamino_program_id());
        assert_eq!(ix.accounts.len(), 11);
        assert_eq!(ix.data.len(), 24);
    }

    #[test]
    fn test_build_save_liquidate_instruction() {
        let dummy = Pubkey::new_unique();
        let params = SaveLiquidateParams {
            source_liquidity: dummy,
            destination_collateral: dummy,
            repay_reserve: dummy,
            repay_reserve_liquidity_supply: dummy,
            withdraw_reserve: dummy,
            withdraw_reserve_collateral_supply: dummy,
            obligation: dummy,
            lending_market: dummy,
            lending_market_authority: dummy,
            user_transfer_authority: dummy,
            token_program: dummy,
            liquidity_amount: 2_000_000,
        };

        let ix = build_save_liquidate_instruction(params);
        assert_eq!(ix.program_id, programs::save_program_id());
        assert_eq!(ix.accounts.len(), 11);
        assert_eq!(ix.data[0], 13u8);
    }
}
