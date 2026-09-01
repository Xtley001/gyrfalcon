//! DEX swap instruction generators for Raydium, Orca Whirlpools, and Meteora DLMM.

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

    pub fn raydium_cp_swap_program_id() -> Pubkey {
        "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C"
            .parse()
            .unwrap()
    }

    pub fn orca_whirlpool_program_id() -> Pubkey {
        "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
            .parse()
            .unwrap()
    }

    pub fn meteora_dlmm_program_id() -> Pubkey {
        "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo"
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
}

/// Build Raydium CLMM swap instruction.
pub fn build_raydium_clmm_swap_instruction(params: RaydiumClmmSwapParams) -> Instruction {
    let mut data = Vec::with_capacity(24);
    // Anchor discriminator: sha256("global:swap")[..8] for Raydium CLMM
    data.extend_from_slice(&[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]);
    data.extend_from_slice(&params.amount_in.to_le_bytes());
    data.extend_from_slice(&params.min_amount_out.to_le_bytes());

    let accounts = vec![
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

/// Parameters for Meteora DLMM swap instruction.
#[derive(Debug, Clone)]
pub struct MeteoraDlmmSwapParams {
    pub lb_pair: Pubkey,
    pub bin_array_bitmap_extension: Option<Pubkey>,
    pub reserve_x: Pubkey,
    pub reserve_y: Pubkey,
    pub user_token_in: Pubkey,
    pub user_token_out: Pubkey,
    pub token_x_mint: Pubkey,
    pub token_y_mint: Pubkey,
    pub oracle: Pubkey,
    pub host_fee_in: Option<Pubkey>,
    pub user: Pubkey,
    pub token_x_program: Pubkey,
    pub token_y_program: Pubkey,
    pub amount_in: u64,
    pub min_amount_out: u64,
}

/// Build Meteora DLMM swap instruction.
pub fn build_meteora_dlmm_swap_instruction(params: MeteoraDlmmSwapParams) -> Instruction {
    let mut data = Vec::with_capacity(24);
    // Anchor discriminator: sha256("global:swap")[..8] for DLMM
    data.extend_from_slice(&[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]);
    data.extend_from_slice(&params.amount_in.to_le_bytes());
    data.extend_from_slice(&params.min_amount_out.to_le_bytes());

    let mut accounts = vec![
        AccountMeta::new(params.lb_pair, false),
        AccountMeta::new(params.reserve_x, false),
        AccountMeta::new(params.reserve_y, false),
        AccountMeta::new(params.user_token_in, false),
        AccountMeta::new(params.user_token_out, false),
        AccountMeta::new_readonly(params.token_x_mint, false),
        AccountMeta::new_readonly(params.token_y_mint, false),
        AccountMeta::new_readonly(params.oracle, false),
        AccountMeta::new_readonly(params.user, true),
        AccountMeta::new_readonly(params.token_x_program, false),
        AccountMeta::new_readonly(params.token_y_program, false),
    ];

    if let Some(bin_ext) = params.bin_array_bitmap_extension {
        accounts.push(AccountMeta::new(bin_ext, false));
    }

    Instruction {
        program_id: dex_programs::meteora_dlmm_program_id(),
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
        };

        let ix = build_raydium_clmm_swap_instruction(params);
        assert_eq!(ix.program_id, dex_programs::raydium_clmm_program_id());
        assert_eq!(ix.accounts.len(), 9);
        assert_eq!(ix.data.len(), 24);
    }
}
