//! Protocol liquidation and flash-loan instruction builders for Kamino Lend
//! and fallback Solend (Save) flash loans.

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

    pub fn spl_token_program_id() -> Pubkey {
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
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
    pub remaining_accounts: Vec<AccountMeta>,
}

/// Build Kamino `LiquidateObligationAndRedeemReserveCollateral` instruction.
pub fn build_kamino_liquidate_instruction(params: KaminoLiquidateParams) -> Instruction {
    // Instruction discriminator for klend::liquidateObligationAndRedeemReserveCollateral
    let mut data = Vec::with_capacity(24);
    // Anchor discriminator: sha256("global:liquidate_obligation_and_redeem_reserve_collateral")[..8]
    data.extend_from_slice(&[0xb1, 0x47, 0x9a, 0xbc, 0xe2, 0x85, 0x4a, 0x37]);
    data.extend_from_slice(&params.liquidity_amount.to_le_bytes());
    data.extend_from_slice(&params.min_acceptable_received_collateral.to_le_bytes());

    let (lending_market_authority, _) = Pubkey::find_program_address(
        &[&params.lending_market.to_bytes()[..32]],
        &programs::kamino_program_id(),
    );

    let mut accounts = vec![
        AccountMeta::new(params.liquidator, true),
        AccountMeta::new(params.obligation, false),
        AccountMeta::new_readonly(params.lending_market, false),
        AccountMeta::new_readonly(lending_market_authority, false),
        AccountMeta::new(params.repay_reserve, false),
        AccountMeta::new(params.repay_reserve_liquidity_supply, false),
        AccountMeta::new(params.withdraw_reserve, false),
        AccountMeta::new(params.withdraw_reserve_collateral_supply, false),
        AccountMeta::new(params.user_source_liquidity, false),
        AccountMeta::new(params.user_destination_collateral, false),
        AccountMeta::new(params.user_destination_liquidity, false),
        AccountMeta::new_readonly(params.token_program, false),
    ];
    accounts.extend(params.remaining_accounts);

    Instruction {
        program_id: programs::kamino_program_id(),
        accounts,
        data,
    }
}

/// Build Flash Loan Borrow instruction.
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

/// Build dedicated Save (Solend) Flash Borrow instruction matching official on-chain layout.
pub fn build_save_flash_borrow_instruction(
    program_id: Pubkey,
    liquidity_amount: u64,
    source_liquidity: Pubkey,
    destination_liquidity: Pubkey,
    reserve: Pubkey,
    lending_market: Pubkey,
) -> Instruction {
    let (lending_market_authority, _) = Pubkey::find_program_address(
        &[&lending_market.to_bytes()[..32]],
        &program_id,
    );

    let mut data = Vec::with_capacity(9);
    data.push(14u8); // Solend FlashBorrowReserveLiquidity tag
    data.extend_from_slice(&liquidity_amount.to_le_bytes());

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_liquidity, false),
            AccountMeta::new(destination_liquidity, false),
            AccountMeta::new(reserve, false),
            AccountMeta::new_readonly(lending_market, false),
            AccountMeta::new_readonly(lending_market_authority, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(programs::spl_token_program_id(), false),
        ],
        data,
    }
}

/// Build dedicated Save (Solend) Flash Repay instruction matching official on-chain layout.
pub fn build_save_flash_repay_instruction(
    program_id: Pubkey,
    liquidity_amount: u64,
    borrow_instruction_index: u8,
    source_liquidity: Pubkey,
    destination_liquidity: Pubkey,
    fee_receiver: Pubkey,
    host_fee_receiver: Pubkey,
    reserve: Pubkey,
    lending_market: Pubkey,
    user_transfer_authority: Pubkey,
) -> Instruction {
    let mut data = Vec::with_capacity(10);
    data.push(15u8); // Solend FlashRepayReserveLiquidity tag
    data.extend_from_slice(&liquidity_amount.to_le_bytes());
    data.push(borrow_instruction_index);

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(source_liquidity, false),
            AccountMeta::new(destination_liquidity, false),
            AccountMeta::new(fee_receiver, false),
            AccountMeta::new(host_fee_receiver, false),
            AccountMeta::new(reserve, false),
            AccountMeta::new_readonly(lending_market, false),
            AccountMeta::new_readonly(user_transfer_authority, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(programs::spl_token_program_id(), false),
        ],
        data,
    }
}

/// Build dedicated Kamino Flash Borrow instruction with Anchor discriminator.
pub fn build_kamino_flash_borrow_instruction(
    program_id: Pubkey,
    liquidity_amount: u64,
    reserve: Pubkey,
    lending_market: Pubkey,
    reserve_source_liquidity: Pubkey,
    user_destination_liquidity: Pubkey,
    token_program: Pubkey,
) -> Instruction {
    let (lending_market_authority, _) = Pubkey::find_program_address(
        &[&lending_market.to_bytes()[..32]],
        &program_id,
    );
    let mut data = Vec::with_capacity(16);
    // Anchor discriminator: sha256("global:flash_borrow_reserve_liquidity")[..8]
    data.extend_from_slice(&[0x87, 0xe7, 0x34, 0xa7, 0x07, 0x34, 0xd4, 0xc1]);
    data.extend_from_slice(&liquidity_amount.to_le_bytes());

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(user_destination_liquidity, false),
            AccountMeta::new(reserve_source_liquidity, false),
            AccountMeta::new(reserve, false),
            AccountMeta::new_readonly(lending_market, false),
            AccountMeta::new_readonly(lending_market_authority, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(token_program, false),
        ],
        data,
    }
}

/// Build dedicated Kamino Flash Repay instruction with Anchor discriminator.
pub fn build_kamino_flash_repay_instruction(
    program_id: Pubkey,
    liquidity_amount: u64,
    borrow_instruction_index: u8,
    reserve: Pubkey,
    lending_market: Pubkey,
    user_source_liquidity: Pubkey,
    reserve_destination_liquidity: Pubkey,
    user_transfer_authority: Pubkey,
    token_program: Pubkey,
) -> Instruction {
    let mut data = Vec::with_capacity(17);
    // Anchor discriminator: sha256("global:flash_repay_reserve_liquidity")[..8]
    data.extend_from_slice(&[0xb9, 0x75, 0x00, 0xcb, 0x60, 0xf5, 0xb4, 0xba]);
    data.extend_from_slice(&liquidity_amount.to_le_bytes());
    data.push(borrow_instruction_index);

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(user_source_liquidity, false),
            AccountMeta::new(reserve_destination_liquidity, false),
            AccountMeta::new(reserve, false),
            AccountMeta::new_readonly(lending_market, false),
            AccountMeta::new_readonly(user_transfer_authority, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(token_program, false),
        ],
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
            remaining_accounts: vec![],
        };

        let ix = build_kamino_liquidate_instruction(params);
        assert_eq!(ix.program_id, programs::kamino_program_id());
        assert_eq!(ix.accounts.len(), 12);
        assert_eq!(ix.data.len(), 24);
        assert_eq!(&ix.data[..8], &[0xb1, 0x47, 0x9a, 0xbc, 0xe2, 0x85, 0x4a, 0x37]);
    }

    #[test]
    fn test_build_save_flash_borrow_and_repay_instructions() {
        let dummy = Pubkey::new_unique();
        let program_id = programs::save_program_id();

        let borrow_ix = build_save_flash_borrow_instruction(
            program_id,
            1_000_000,
            dummy,
            dummy,
            dummy,
            dummy,
        );
        assert_eq!(borrow_ix.program_id, program_id);
        assert_eq!(borrow_ix.accounts.len(), 7);
        assert_eq!(borrow_ix.data[0], 14u8);
        assert_eq!(&borrow_ix.data[1..9], &1_000_000u64.to_le_bytes());

        let repay_ix = build_save_flash_repay_instruction(
            program_id,
            1_000_300, // 1M + 300 fee
            0,
            dummy,
            dummy,
            dummy,
            dummy,
            dummy,
            dummy,
            dummy,
        );
        assert_eq!(repay_ix.program_id, program_id);
        assert_eq!(repay_ix.accounts.len(), 9);
        assert_eq!(repay_ix.data[0], 15u8);
        assert_eq!(&repay_ix.data[1..9], &1_000_300u64.to_le_bytes());
        assert_eq!(repay_ix.data[9], 0u8);
    }

    #[test]
    fn test_build_kamino_flash_borrow_and_repay_instructions() {
        let dummy = Pubkey::new_unique();
        let program_id = programs::kamino_program_id();

        let borrow_ix = build_kamino_flash_borrow_instruction(
            program_id,
            5_000_000,
            dummy,
            dummy,
            dummy,
            dummy,
            dummy,
        );
        assert_eq!(borrow_ix.program_id, program_id);
        assert_eq!(borrow_ix.accounts.len(), 7);
        assert_eq!(&borrow_ix.data[..8], &[0x87, 0xe7, 0x34, 0xa7, 0x07, 0x34, 0xd4, 0xc1]);

        let repay_ix = build_kamino_flash_repay_instruction(
            program_id,
            5_001_500,
            1,
            dummy,
            dummy,
            dummy,
            dummy,
            dummy,
            dummy,
        );
        assert_eq!(repay_ix.program_id, program_id);
        assert_eq!(repay_ix.accounts.len(), 7);
        assert_eq!(&repay_ix.data[..8], &[0xb9, 0x75, 0x00, 0xcb, 0x60, 0xf5, 0xb4, 0xba]);
    }
}
