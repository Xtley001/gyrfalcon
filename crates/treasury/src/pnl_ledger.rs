//! Realized PnL ledger — feeds the drawdown circuit breaker
//! (`docs/STRATEGY.md#circuit-breakers`: "Realized 24h PnL below
//! `risk.max_drawdown_usd`"). This module only tracks and sums realized
//! outcomes; `gyrfalcon_strategy::breakers::BreakerState` is what actually
//! trips the breaker — this is the number that check is computed from.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub slot: u64,
    pub net_usd: f64,
}

/// A rolling window of realized net PnL entries, in **slots** rather than
/// wall-clock time — this module has no calendar/clock dependency, and
/// slots are what every other timing decision in this codebase (sync lag,
/// consecutive reverts) is already denominated in. The caller supplies a
/// slots-per-window figure (e.g. Solana's ~2.5 slots/sec puts a 24h window
/// at roughly 216,000 slots) rather than this module hardcoding a
/// network-specific slot time.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PnlLedger {
    entries: VecDeque<Entry>,
}

impl PnlLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one realized outcome (a landed liquidation's actual net
    /// profit, which may be negative — a landed-but-reverted-elsewhere
    /// edge case, or a tip paid on a lost race, still costs money and
    /// belongs in the drawdown figure).
    pub fn record(&mut self, slot: u64, net_usd: f64) {
        self.entries.push_back(Entry { slot, net_usd });
    }

    /// Sum of realized PnL for entries within `window_slots` of
    /// `current_slot`. Older entries are not evicted from internal
    /// storage by this call — see [`PnlLedger::prune_older_than`] for
    /// that, kept as a separate explicit step so a read never has the
    /// side effect of mutating history.
    pub fn windowed_sum(&self, current_slot: u64, window_slots: u64) -> f64 {
        let cutoff = current_slot.saturating_sub(window_slots);
        self.entries
            .iter()
            .filter(|e| e.slot >= cutoff && e.slot <= current_slot)
            .map(|e| e.net_usd)
            .sum()
    }

    /// Drop entries older than `current_slot - window_slots` so the
    /// ledger doesn't grow unboundedly over a long-running process. Call
    /// periodically, not on every read.
    pub fn prune_older_than(&mut self, current_slot: u64, window_slots: u64) {
        let cutoff = current_slot.saturating_sub(window_slots);
        while let Some(front) = self.entries.front() {
            if front.slot < cutoff {
                self.entries.pop_front();
            } else {
                break;
            }
        }
    }

    /// Save realized PnL entries to disk as JSON to survive process restarts.
    pub fn snapshot(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let path = path.as_ref();
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Restore realized PnL entries from a disk snapshot.
    pub fn restore(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path)?;
        let entries: VecDeque<Entry> = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Checks whether the current windowed PnL has crossed
/// `risk.max_drawdown_usd`. `max_drawdown_usd` is a magnitude (STRATEGY.md
/// gives it a value like `1000.0`, not `-1000.0`) — a breach is
/// `windowed_sum <= -max_drawdown_usd`.
pub fn drawdown_breached(windowed_sum_usd: f64, max_drawdown_usd: f64) -> bool {
    windowed_sum_usd <= -max_drawdown_usd.abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windowed_sum_excludes_entries_outside_the_window() {
        let mut ledger = PnlLedger::new();
        ledger.record(100, 50.0); // outside window at current_slot=1000, window=500
        ledger.record(600, 30.0); // inside
        ledger.record(900, -10.0); // inside

        let sum = ledger.windowed_sum(1000, 500);
        assert_eq!(sum, 20.0); // 30 - 10, the slot-100 entry excluded
    }

    #[test]
    fn prune_removes_only_entries_older_than_the_window() {
        let mut ledger = PnlLedger::new();
        ledger.record(100, 1.0);
        ledger.record(900, 1.0);
        ledger.prune_older_than(1000, 500); // cutoff = 500
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn drawdown_breached_uses_absolute_value_of_the_configured_limit() {
        assert!(drawdown_breached(-1000.0, 1000.0));
        assert!(drawdown_breached(-1500.0, 1000.0));
        assert!(!drawdown_breached(-999.0, 1000.0));
        assert!(!drawdown_breached(500.0, 1000.0)); // profitable, never breached
    }

    #[test]
    fn test_snapshot_restore_roundtrip() {
        let mut ledger = PnlLedger::new();
        ledger.record(100, 25.5);
        ledger.record(200, -10.2);

        let dir = std::env::temp_dir().join(format!("pnl_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pnl_ledger.json");

        ledger.snapshot(&path).unwrap();
        let restored = PnlLedger::restore(&path).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.windowed_sum(250, 200), 15.3);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
