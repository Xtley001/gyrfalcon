//! DEX swap instruction generators for Raydium CLMM, Orca Whirlpool,
//! Sanctum (JitoSOL/SOL), and Marinade (mSOL/SOL) per `03_ROUTING_DEX.md`.

use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;

/// Well-known AMM and DEX program IDs on Solana Mainnet-Beta.
pub mod dex_programs {
    use solana_sdk::pubkey::Pubkey;

    pub fn raydium_clmm_program_id() -> Pubkey {
        "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK"
            .parse()
            .unwrap()
    }

    pub fn orca_whirlpool_program_id() -> Pubkey {
        "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
            .parse()
            .unwrap()
    }

    pub fn sanctum_program_id() -> Pubkey {
        "5ocnV1qiCgaQR8Jb8xWnVbApNzpWCDveWUig21uT3J9z"
            .parse()
            .unwrap()
    }

    pub fn marinade_program_id() -> Pubkey {
        "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD"
            .parse()
            .unwrap()
    }
}

/// Parameters for Raydium CLMM swap instruction.
#[derive(Debug, Clone)]
pub struct RaydiumClmmSwapParams {
    pub clmm_program: Pubkey,
    pub payer: Pubkey,
    pub pool_state: Pubkey,
    pub input_token_account: Pubkey,
    pub output_token_account: Pubkey,
    pub input_vault: Pubkey,
    pub output_vault: Pubkey,
    pub observation_state: Pubkey,
    pub token_program: Pubkey,
    pub amount_in: u64,
    pub min_amount_out: u64,
    pub amm_config: Option<Pubkey>,
    pub tick_arrays: Vec<Pubkey>,
}

/// Build Raydium CLMM swap instruction.
pub fn build_raydium_clmm_swap_instruction(params: RaydiumClmmSwapParams) -> Instruction {
    let mut data = Vec::with_capacity(24);
    // Anchor discriminator: sha256("global:swap")[..8] for Raydium CLMM
    data.extend_from_slice(&[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]);
    data.extend_from_slice(&params.amount_in.to_le_bytes());
    data.extend_from_slice(&params.min_amount_out.to_le_bytes());

    let mut accounts = vec![
        AccountMeta::new_readonly(params.payer, true),
        AccountMeta::new_readonly(params.clmm_program, false),
        AccountMeta::new(params.pool_state, false),
        AccountMeta::new(params.input_token_account, false),
        AccountMeta::new(params.output_token_account, false),
        AccountMeta::new(params.input_vault, false),
        AccountMeta::new(params.output_vault, false),
        AccountMeta::new(params.observation_state, false),
        AccountMeta::new_readonly(params.token_program, false),
    ];

    if let Some(config) = params.amm_config {
        accounts.push(AccountMeta::new_readonly(config, false));
    }
    for tick_array in params.tick_arrays {
        accounts.push(AccountMeta::new(tick_array, false));
    }

    Instruction {
        program_id: dex_programs::raydium_clmm_program_id(),
        accounts,
        data,
    }
}

/// Parameters for Orca Whirlpool swap instruction.
#[derive(Debug, Clone)]
pub struct OrcaWhirlpoolSwapParams {
    pub whirlpool_program: Pubkey,
    pub token_authority: Pubkey,
    pub whirlpool: Pubkey,
    pub token_owner_account_a: Pubkey,
    pub token_vault_a: Pubkey,
    pub token_owner_account_b: Pubkey,
    pub token_vault_b: Pubkey,
    pub tick_array_0: Pubkey,
    pub tick_array_1: Pubkey,
    pub tick_array_2: Pubkey,
    pub oracle: Pubkey,
    pub token_program: Pubkey,
    pub amount: u64,
    pub other_amount_threshold: u64,
    pub sqrt_price_limit: u128,
    pub amount_specified_is_input: bool,
    pub a_to_b: bool,
}

/// Build Orca Whirlpool swap instruction.
pub fn build_orca_whirlpool_swap_instruction(params: OrcaWhirlpoolSwapParams) -> Instruction {
    let mut data = Vec::with_capacity(40);
    // Anchor discriminator: sha256("global:swap")[..8] for Orca Whirlpool
    data.extend_from_slice(&[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]);
    data.extend_from_slice(&params.amount.to_le_bytes());
    data.extend_from_slice(&params.other_amount_threshold.to_le_bytes());
    data.extend_from_slice(&params.sqrt_price_limit.to_le_bytes());
    data.push(params.amount_specified_is_input as u8);
    data.push(params.a_to_b as u8);

    let accounts = vec![
        AccountMeta::new_readonly(params.token_program, false),
        AccountMeta::new_readonly(params.token_authority, true),
        AccountMeta::new(params.whirlpool, false),
        AccountMeta::new(params.token_owner_account_a, false),
        AccountMeta::new(params.token_vault_a, false),
        AccountMeta::new(params.token_owner_account_b, false),
        AccountMeta::new(params.token_vault_b, false),
        AccountMeta::new(params.tick_array_0, false),
        AccountMeta::new(params.tick_array_1, false),
        AccountMeta::new(params.tick_array_2, false),
        AccountMeta::new_readonly(params.oracle, false),
    ];

    Instruction {
        program_id: dex_programs::orca_whirlpool_program_id(),
        accounts,
        data,
    }
}

/// Parameters for Sanctum swap / unstake instruction (JitoSOL -> SOL redemption).
#[derive(Debug, Clone)]
pub struct SanctumSwapParams {
    pub sanctum_program: Pubkey,
    pub user: Pubkey,
    pub src_token_account: Pubkey,
    pub dst_token_account: Pubkey,
    pub pool: Pubkey,
    pub token_program: Pubkey,
    pub amount_in: u64,
    pub min_amount_out: u64,
}

/// Build Sanctum swap/unstake instruction for JitoSOL -> SOL redemption.
pub fn build_sanctum_swap_instruction(params: SanctumSwapParams) -> Instruction {
    let mut data = Vec::with_capacity(24);
    // Anchor discriminator: sha256("global:swap")[..8]
    data.extend_from_slice(&[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]);
    data.extend_from_slice(&params.amount_in.to_le_bytes());
    data.extend_from_slice(&params.min_amount_out.to_le_bytes());

    let accounts = vec![
        AccountMeta::new_readonly(params.user, true),
        AccountMeta::new(params.pool, false),
        AccountMeta::new(params.src_token_account, false),
        AccountMeta::new(params.dst_token_account, false),
        AccountMeta::new_readonly(params.token_program, false),
    ];

    Instruction {
        program_id: dex_programs::sanctum_program_id(),
        accounts,
        data,
    }
}

/// Parameters for Marinade liquid unstake / swap instruction (mSOL -> SOL redemption).
#[derive(Debug, Clone)]
pub struct MarinadeSwapParams {
    pub marinade_program: Pubkey,
    pub state: Pubkey,
    pub msol_mint: Pubkey,
    pub liq_pool_sol_leg_pda: Pubkey,
    pub liq_pool_msol_leg: Pubkey,
    pub liq_pool_msol_leg_authority: Pubkey,
    pub transfer_from: Pubkey,
    pub transfer_to: Pubkey,
    pub transfer_from_authority: Pubkey,
    pub token_program: Pubkey,
    pub msol_amount: u64,
    pub min_sol_receive: u64,
}

/// Build Marinade liquid unstake instruction for mSOL -> SOL redemption.
pub fn build_marinade_swap_instruction(params: MarinadeSwapParams) -> Instruction {
    let mut data = Vec::with_capacity(17);
    // Marinade liquid_unstake instruction tag: 14u8
    data.push(14u8);
    data.extend_from_slice(&params.msol_amount.to_le_bytes());
    data.extend_from_slice(&params.min_sol_receive.to_le_bytes());

    let accounts = vec![
        AccountMeta::new(params.state, false),
        AccountMeta::new(params.msol_mint, false),
        AccountMeta::new(params.liq_pool_sol_leg_pda, false),
        AccountMeta::new(params.liq_pool_msol_leg, false),
        AccountMeta::new_readonly(params.liq_pool_msol_leg_authority, false),
        AccountMeta::new(params.transfer_from, false),
        AccountMeta::new(params.transfer_to, false),
        AccountMeta::new_readonly(params.transfer_from_authority, true),
        AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        AccountMeta::new_readonly(params.token_program, false),
    ];

    Instruction {
        program_id: dex_programs::marinade_program_id(),
        accounts,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_raydium_clmm_swap_instruction() {
        let dummy = Pubkey::new_unique();
        let params = RaydiumClmmSwapParams {
            clmm_program: dummy,
            payer: dummy,
            pool_state: dummy,
            input_token_account: dummy,
            output_token_account: dummy,
            input_vault: dummy,
            output_vault: dummy,
            observation_state: dummy,
            token_program: dummy,
            amount_in: 1_000_000,
            min_amount_out: 990_000,
            amm_config: None,
            tick_arrays: vec![],
        };

        let ix = build_raydium_clmm_swap_instruction(params);
        assert_eq!(ix.program_id, dex_programs::raydium_clmm_program_id());
        assert_eq!(ix.accounts.len(), 9);
        assert_eq!(ix.data.len(), 24);
    }

    #[test]
    fn test_build_orca_whirlpool_swap_instruction() {
        let dummy = Pubkey::new_unique();
        let params = OrcaWhirlpoolSwapParams {
            whirlpool_program: dex_programs::orca_whirlpool_program_id(),
            token_authority: dummy,
            whirlpool: dummy,
            token_owner_account_a: dummy,
            token_vault_a: dummy,
            token_owner_account_b: dummy,
            token_vault_b: dummy,
            tick_array_0: dummy,
            tick_array_1: dummy,
            tick_array_2: dummy,
            oracle: dummy,
            token_program: dummy,
            amount: 500_000,
            other_amount_threshold: 495_000,
            sqrt_price_limit: 0,
            amount_specified_is_input: true,
            a_to_b: true,
        };

        let ix = build_orca_whirlpool_swap_instruction(params);
        assert_eq!(ix.program_id, dex_programs::orca_whirlpool_program_id());
        assert_eq!(ix.accounts.len(), 11);
        assert_eq!(ix.data.len(), 42);
    }

    #[test]
    fn test_build_sanctum_swap_instruction() {
        let dummy = Pubkey::new_unique();
        let params = SanctumSwapParams {
            sanctum_program: dex_programs::sanctum_program_id(),
            user: dummy,
            src_token_account: dummy,
            dst_token_account: dummy,
            pool: dummy,
            token_program: dummy,
            amount_in: 2_000_000,
            min_amount_out: 1_995_000,
        };

        let ix = build_sanctum_swap_instruction(params);
        assert_eq!(ix.program_id, dex_programs::sanctum_program_id());
        assert_eq!(ix.accounts.len(), 5);
        assert_eq!(ix.data.len(), 24);
    }

    #[test]
    fn test_build_marinade_swap_instruction() {
        let dummy = Pubkey::new_unique();
        let params = MarinadeSwapParams {
            marinade_program: dex_programs::marinade_program_id(),
            state: dummy,
            msol_mint: dummy,
            liq_pool_sol_leg_pda: dummy,
            liq_pool_msol_leg: dummy,
            liq_pool_msol_leg_authority: dummy,
            transfer_from: dummy,
            transfer_to: dummy,
            transfer_from_authority: dummy,
            token_program: dummy,
            msol_amount: 3_000_000,
            min_sol_receive: 2_990_000,
        };

        let ix = build_marinade_swap_instruction(params);
        assert_eq!(ix.program_id, dex_programs::marinade_program_id());
        assert_eq!(ix.accounts.len(), 10);
        assert_eq!(ix.data.len(), 17);
        assert_eq!(ix.data[0], 14u8);
    }
}
