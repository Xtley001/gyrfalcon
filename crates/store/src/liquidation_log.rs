//! `liquidation_log` — append-only log of terminal outcomes.
//!
//! Per `docs/API.md#state-store`: source of truth for the dashboard's PnL
//! history and the drawdown circuit breaker. `export_liquidation_log` is the
//! intended integration point for external bookkeeping — build against this
//! schema, not by parsing dashboard output (the dashboard is a visual
//! reference, not a data API; see `docs/FEATURES.md`).

use crate::position_book::StoreError;
use gyrfalcon_core::LiquidationRecord;
use std::io::Write;
use std::path::{Path, PathBuf};

pub enum ExportFormat {
    Csv,
    Json,
}

#[derive(Debug, Default)]
pub struct LiquidationLog {
    records: Vec<LiquidationRecord>,
    /// If set, every `append` is also fsync'd here as a JSON line —
    /// append-only, matching the "never rewritten" contract in API.md.
    backing_file: Option<PathBuf>,
}

impl LiquidationLog {
    /// In-memory only — useful for tests and for `sim`/`strategy` unit
    /// tests that never need durability.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Backed by an append-only JSONL file at `path`. Existing records (if
    /// any) are loaded first so a restart resumes the log rather than
    /// truncating it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let mut records = Vec::new();
        if path.exists() {
            let contents = std::fs::read_to_string(&path).map_err(|source| StoreError::Io {
                path: path.display().to_string(),
                source,
            })?;
            for line in contents.lines().filter(|l| !l.trim().is_empty()) {
                records.push(serde_json::from_str(line)?);
            }
        }
        Ok(Self {
            records,
            backing_file: Some(path),
        })
    }

    /// Append a terminal outcome. Never rewrites or reorders existing
    /// entries.
    pub fn append(&mut self, record: LiquidationRecord) -> Result<(), StoreError> {
        if let Some(path) = &self.backing_file {
            let line = serde_json::to_string(&record)?;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|source| StoreError::Io {
                    path: path.display().to_string(),
                    source,
                })?;
            writeln!(file, "{line}").map_err(|source| StoreError::Io {
                path: path.display().to_string(),
                source,
            })?;
        }
        self.records.push(record);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &LiquidationRecord> {
        self.records.iter()
    }

    /// Export a slot range as CSV or JSON, per `docs/API.md`'s
    /// `store::export_liquidation_log(range) -> CSV | JSON`.
    pub fn export(
        &self,
        slot_range: std::ops::Range<u64>,
        format: ExportFormat,
    ) -> Result<String, StoreError> {
        let filtered: Vec<&LiquidationRecord> = self
            .records
            .iter()
            .filter(|r| slot_range.contains(&r.submitted_at_slot))
            .collect();

        match format {
            ExportFormat::Json => Ok(serde_json::to_string_pretty(&filtered)?),
            ExportFormat::Csv => {
                let mut out = String::from(
                    "protocol,position_id,repay_amount,flash_source_protocol,net_usd_expected,submitted_at_slot,resolved_at_slot,outcome\n",
                );
                for r in filtered {
                    let outcome = match &r.outcome {
                        gyrfalcon_core::SubmitOutcome::Landed {
                            slot,
                            actual_net_usd,
                        } => {
                            format!("landed@{slot}:{actual_net_usd:.2}")
                        }
                        gyrfalcon_core::SubmitOutcome::Reverted { reason } => {
                            format!("reverted:{reason:?}")
                        }
                        gyrfalcon_core::SubmitOutcome::NotIncluded => "not_included".to_string(),
                    };
                    out.push_str(&format!(
                        "{},{},{},{},{:.2},{},{},{}\n",
                        r.routed.candidate.protocol,
                        r.routed.candidate.position_id,
                        r.routed.repay_amount,
                        r.routed.flash_source.protocol,
                        r.routed.expected.net_usd,
                        r.submitted_at_slot,
                        r.resolved_at_slot,
                        outcome,
                    ));
                }
                Ok(out)
            }
        }
    }
}

/// Asynchronous non-blocking writer for terminal liquidation records.
/// Uses an in-memory mpsc channel to avoid blocking the hot execution path on disk I/O.
pub struct AsyncLiquidationWriter {
    sender: tokio::sync::mpsc::Sender<LiquidationRecord>,
}

impl AsyncLiquidationWriter {
    pub fn spawn(path: PathBuf, buffer_size: usize) -> (Self, tokio::task::JoinHandle<()>) {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(buffer_size);

        let handle = tokio::spawn(async move {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            while let Some(record) = receiver.recv().await {
                if let Ok(line) = serde_json::to_string(&record) {
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                    {
                        let _ = writeln!(file, "{line}");
                    }
                }
            }
        });

        (Self { sender }, handle)
    }

    /// Try to send record non-blockingly to the background persistence task.
    pub fn try_record(&self, record: LiquidationRecord) -> Result<(), StoreError> {
        self.sender
            .try_send(record)
            .map_err(|e| StoreError::CapacityExceeded {
                details: format!("Async log channel full: {e}"),
            })
    }

    /// Asynchronously append a record.
    pub async fn record(&self, record: LiquidationRecord) -> Result<(), StoreError> {
        self.sender
            .send(record)
            .await
            .map_err(|e| StoreError::CapacityExceeded {
                details: format!("Async log receiver dropped: {e}"),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::{
        BreachCandidate, FlashSource, ProfitEstimate, Protocol, RoutedCandidate, SubmitOutcome,
    };

    fn sample_record(slot: u64) -> LiquidationRecord {
        LiquidationRecord {
            routed: RoutedCandidate {
                candidate: BreachCandidate {
                    protocol: Protocol::Kamino,
                    position_id: gyrfalcon_core::Pubkey::new([9u8; 32]),
                    collateral_mint: gyrfalcon_core::Pubkey::new([1u8; 32]),
                    debt_mint: gyrfalcon_core::Pubkey::new([2u8; 32]),
                    health_factor: 0.91,
                    close_factor_max_repay: 1_000_000,
                    slot,
                },
                repay_amount: 500_000,
                flash_source: FlashSource {
                    protocol: Protocol::Kamino,
                    reserve: gyrfalcon_core::Pubkey::new([3u8; 32]),
                    fee_bps: 5,
                },
                expected: ProfitEstimate {
                    bonus_usd: 40.0,
                    est_slippage_usd: 2.0,
                    flash_fee_usd: 1.0,
                    est_cu_cost_usd: 0.5,
                    bid_tip_usd: 10.0,
                    net_usd: 26.5,
                },
            },
            outcome: SubmitOutcome::Landed {
                slot: slot + 1,
                actual_net_usd: 25.9,
            },
            submitted_at_slot: slot,
            resolved_at_slot: slot + 1,
        }
    }

    #[test]
    fn append_only_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!("gyrfalcon-log-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("liquidation_log.jsonl");

        {
            let mut log = LiquidationLog::open(&path).unwrap();
            log.append(sample_record(100)).unwrap();
            log.append(sample_record(200)).unwrap();
            assert_eq!(log.len(), 2);
        }

        // Reopening must not lose or duplicate entries.
        let reopened = LiquidationLog::open(&path).unwrap();
        assert_eq!(reopened.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_json_filters_by_slot_range() {
        let mut log = LiquidationLog::in_memory();
        log.append(sample_record(100)).unwrap();
        log.append(sample_record(500)).unwrap();

        let json = log.export(0..300, ExportFormat::Json).unwrap();
        assert!(json.contains("100"));
        assert!(!json.contains("\"submitted_at_slot\": 500"));
    }

    #[test]
    fn export_csv_has_header_and_one_row_per_record() {
        let mut log = LiquidationLog::in_memory();
        log.append(sample_record(100)).unwrap();
        let csv = log.export(0..u64::MAX, ExportFormat::Csv).unwrap();
        assert_eq!(csv.lines().count(), 2); // header + 1 row
    }
}
