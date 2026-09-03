//! `position_book` — latest known state per tracked position, per protocol.
//!
//! Per `docs/API.md#state-store`: overwritten on every account update, and
//! periodically snapshotted to disk purely so a restart doesn't force a cold
//! re-sync from genesis-of-subscription. The in-memory map is authoritative
//! during normal operation — the snapshot is a recovery aid, not a source of
//! truth.

use gyrfalcon_core::{Protocol, Pubkey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionRecord {
    pub protocol: Protocol,
    pub position_id: Pubkey,
    pub collateral_mint: Pubkey,
    pub debt_mint: Pubkey,
    pub health_factor: f64,
    pub last_updated_slot: u64,
}

#[derive(Debug, Default)]
pub struct PositionBook {
    positions: HashMap<Pubkey, PositionRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to (de)serialize snapshot: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("store capacity exceeded: {details}")]
    CapacityExceeded { details: String },
}

impl PositionBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overwrite (or insert) the latest known state for a position.
    pub fn upsert(&mut self, record: PositionRecord) {
        self.positions.insert(record.position_id, record);
    }

    pub fn get(&self, position_id: &Pubkey) -> Option<&PositionRecord> {
        self.positions.get(position_id)
    }

    pub fn remove(&mut self, position_id: &Pubkey) -> Option<PositionRecord> {
        self.positions.remove(position_id)
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PositionRecord> {
        self.positions.values()
    }

    /// Write every tracked position to `path` as JSON. Intended to run on a
    /// periodic timer, not on the hot path.
    pub fn snapshot(&self, path: impl AsRef<Path>) -> Result<(), StoreError> {
        let path = path.as_ref();
        let records: Vec<&PositionRecord> = self.positions.values().collect();
        let json = serde_json::to_string_pretty(&records)?;
        std::fs::write(path, json).map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    /// Restore from a snapshot written by [`PositionBook::snapshot`]. Used on
    /// process restart to avoid a cold re-sync from genesis-of-subscription.
    pub fn restore(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path).map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let records: Vec<PositionRecord> = serde_json::from_str(&json)?;
        let mut book = Self::new();
        for record in records {
            book.upsert(record);
        }
        Ok(book)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id_byte: u8, slot: u64) -> PositionRecord {
        PositionRecord {
            protocol: Protocol::Kamino,
            position_id: Pubkey::new([id_byte; 32]),
            collateral_mint: Pubkey::new([1u8; 32]),
            debt_mint: Pubkey::new([2u8; 32]),
            health_factor: 0.97,
            last_updated_slot: slot,
        }
    }

    #[test]
    fn upsert_overwrites_by_position_id() {
        let mut book = PositionBook::new();
        book.upsert(sample(1, 100));
        book.upsert(sample(1, 200));
        assert_eq!(book.len(), 1);
        assert_eq!(
            book.get(&Pubkey::new([1u8; 32])).unwrap().last_updated_slot,
            200
        );
    }

    #[test]
    fn snapshot_restore_round_trip_loses_nothing() {
        let mut book = PositionBook::new();
        for i in 0..25u8 {
            book.upsert(sample(i, i as u64 * 10));
        }

        let dir = std::env::temp_dir().join(format!("gyrfalcon-store-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("position_book.json");

        book.snapshot(&path).unwrap();
        let restored = PositionBook::restore(&path).unwrap();

        assert_eq!(restored.len(), book.len());
        for original in book.iter() {
            let restored_record = restored.get(&original.position_id).unwrap();
            assert_eq!(restored_record, original);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restore_missing_file_errors() {
        let err = PositionBook::restore("/nonexistent/position_book.json").unwrap_err();
        assert!(matches!(err, StoreError::Io { .. }));
    }
}
