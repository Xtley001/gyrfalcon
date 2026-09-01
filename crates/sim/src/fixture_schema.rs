//! Schema for `tests/fixtures/liquidations.jsonl`, per
//! [`tests/fixtures/README.md`](../../../tests/fixtures/README.md) and
//! [`docs/TESTING.md#historical-replay`](../../../docs/TESTING.md).
//!
//! **This file ships with zero real fixture data.** Per
//! `tests/fixtures/README.md`, `liquidations.jsonl` is produced by "a
//! one-time historical export step (pull real past liquidation events for
//! Kamino, Save, and MarginFi from on-chain history)" — that step needs
//! live Solana RPC/archive access this environment does not have, and per
//! `docs/DATA_POLICY.md` this codebase does not fabricate liquidation data
//! to fill the gap. What's here is the harness that will consume that file
//! once it exists, proven against clearly-labeled synthetic events in this
//! crate's own tests — never against anything checked in as `liquidations.jsonl`
//! itself.

use gyrfalcon_core::Pubkey;
use serde::{Deserialize, Serialize};

/// One historical liquidation event, one per line of `liquidations.jsonl`.
///
/// Field-for-field, this needs to carry enough information to answer the
/// replay harness's three questions per `docs/TESTING.md`:
/// - **Detection** — was `health_factor_at_breach_slot` (and the slot it
///   was observed at) something our decoded `Obligation` would also have
///   produced from the raw account bytes at that slot?
/// - **Routing** — did the flash-source reserve we'd have picked actually
///   have `available_liquidity_at_slot` >= the repay amount, at that
///   historical moment (not current/headline TVL)?
/// - **Feasibility** — does the route fit under the CU/byte ceilings
///   (checked against the Stage 7 CU table, not re-derived here)?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalLiquidationEvent {
    pub protocol: gyrfalcon_core::Protocol,
    pub position_id: Pubkey,
    pub collateral_mint: Pubkey,
    pub debt_mint: Pubkey,
    /// Slot at which the position first crossed its liquidation threshold.
    pub breach_slot: u64,
    /// Health factor computed from the raw account state at `breach_slot`
    /// by the historical export step — this is what a correct decoder
    /// should reproduce, independent of gyrfalcon's own code.
    pub health_factor_at_breach_slot: f64,
    /// Raw account bytes for the obligation at `breach_slot`, base64.
    /// Empty in a manually-authored fixture; a real export includes this
    /// so `on_account_update` can be replayed against real bytes rather
    /// than a pre-computed number.
    pub obligation_data_base64: String,
    pub obligation_owner: Pubkey,
    /// The reserve that actually had the deepest liquidity at
    /// `breach_slot`, and how much, per the historical export.
    pub best_flash_reserve: Pubkey,
    pub best_flash_reserve_available_liquidity: u64,
    /// The slot the real liquidation transaction landed at, if it landed —
    /// used to check detection timing (breach_slot -> landed_slot gap).
    pub landed_slot: Option<u64>,
}

use base64::prelude::*;

/// Standard base64 decoder for raw account state fixtures.
pub fn decode_base64(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    BASE64_STANDARD.decode(s.trim()).ok()
}

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("malformed event on line {line_no}: {source}")]
    Json {
        line_no: usize,
        #[source]
        source: serde_json::Error,
    },
}

/// Load `liquidations.jsonl`. Returns an empty `Vec` (not an error) when
/// the file exists but is empty — an empty replay set is a valid, if
/// uninformative, state; a *missing* file is treated the same way, since
/// per `tests/fixtures/README.md` its absence just means the one-time
/// export step hasn't been run yet, which is expected in this repo as
/// shipped.
pub fn load_events(
    path: impl AsRef<std::path::Path>,
) -> Result<Vec<HistoricalLiquidationEvent>, FixtureError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(path).map_err(|source| FixtureError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut events = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str(line).map_err(|source| FixtureError::Json {
            line_no: i + 1,
            source,
        })?;
        events.push(event);
    }
    Ok(events)
}
