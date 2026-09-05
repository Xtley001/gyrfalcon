//! `cargo run --bin export-fixtures`
//!
//! Generates the liquidation event fixtures into `tests/fixtures/liquidations.jsonl`.

use gyrfalcon_core::{Protocol, Pubkey};
use gyrfalcon_sim::HistoricalLiquidationEvent;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_path = Path::new("tests/fixtures/liquidations.jsonl");

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = File::create(output_path)?;

    // Mint constants
    let sol_mint = Pubkey([1u8; 32]);
    let usdc_mint = Pubkey([2u8; 32]);

    // Kamino fixtures
    let kamino_event = HistoricalLiquidationEvent {
        protocol: Protocol::Kamino,
        position_id: Pubkey([10u8; 32]),
        collateral_mint: sol_mint,
        debt_mint: usdc_mint,
        breach_slot: 290_000_100,
        health_factor_at_breach_slot: 0.94,
        obligation_data_base64: String::new(),
        obligation_owner: Pubkey([11u8; 32]),
        best_flash_reserve: Pubkey([12u8; 32]),
        best_flash_reserve_available_liquidity: 50_000_000_000,
        landed_slot: Some(290_000_102),
    };

    let events = vec![kamino_event];

    for event in &events {
        let serialized = serde_json::to_string(event)?;
        writeln!(file, "{serialized}")?;
    }

    println!("Successfully exported {} events to {}", events.len(), output_path.display());
    Ok(())
}
