//! Sweep scheduling and minimum-balance check —
//! `docs/STRATEGY.md#treasury-and-capital-management`: "Realized profit...
//! is swept from the settlement wallet back to the treasury on a
//! schedule," and "Below `treasury.min_balance_sol`, the engine stops
//! firing new candidates and alerts."
//!
//! Neither of these talk to a real wallet or RPC endpoint — this build
//! has no live Solana RPC access, so there's no live balance to query.
//! Both functions take the current balance/elapsed time as plain
//! arguments; wiring an actual `solana-client` balance query and a real
//! sweep transaction is Stage B+/C work once RPC access exists to build
//! and test against.

/// True once `current_balance_sol` has dropped below
/// `treasury.min_balance_sol` — STRATEGY.md's minimum-balance breaker
/// trigger. Named distinctly from `strategy::breakers::BreakerState`'s
/// `treasury_below_floor` flag (which this function's result feeds) so
/// the "is it below floor right now" check and "is the engine currently
/// halted for that reason" state stay separate — the breaker only clears
/// automatically once this recomputes `false` on a fresh balance read.
pub fn is_below_minimum_balance(current_balance_sol: f64, min_balance_sol: f64) -> bool {
    current_balance_sol < min_balance_sol
}

/// Parses `treasury.sweep_interval` (`docs/CONFIGURATION.md`: e.g. `"1h"`,
/// `"30m"`) into a slot count, given a network's approximate slot time.
/// Solana's slot time isn't perfectly constant, so this is an
/// approximation good enough for a periodic sweep — a sweep firing a few
/// slots early or late has no correctness impact, unlike the sync-lag or
/// consecutive-revert breakers.
pub fn parse_sweep_interval_slots(
    interval: &str,
    avg_slot_ms: u64,
) -> Result<u64, SweepIntervalError> {
    if interval.is_empty() {
        return Err(SweepIntervalError::Empty);
    }
    let (num_str, unit) = interval.split_at(interval.len() - 1);
    let value: u64 = num_str
        .parse()
        .map_err(|_| SweepIntervalError::Malformed(interval.to_string()))?;
    let seconds = match unit {
        "s" => value,
        "m" => value * 60,
        "h" => value * 3600,
        "d" => value * 86_400,
        other => return Err(SweepIntervalError::UnknownUnit(other.to_string())),
    };
    let ms = seconds.saturating_mul(1000);
    Ok(ms / avg_slot_ms.max(1))
}

#[derive(Debug, thiserror::Error)]
pub enum SweepIntervalError {
    #[error("sweep_interval is empty")]
    Empty,
    #[error("could not parse numeric part of sweep_interval {0:?}")]
    Malformed(String),
    #[error("unknown sweep_interval unit {0:?} — expected one of s, m, h, d")]
    UnknownUnit(String),
}

/// Should a sweep fire now, given the slot of the last sweep and the
/// interval computed by [`parse_sweep_interval_slots`]?
pub fn sweep_due(current_slot: u64, last_swept_slot: u64, interval_slots: u64) -> bool {
    current_slot.saturating_sub(last_swept_slot) >= interval_slots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_minimum_balance_is_strict_less_than() {
        assert!(is_below_minimum_balance(0.5, 1.0));
        assert!(!is_below_minimum_balance(1.0, 1.0)); // exactly at floor is not below it
        assert!(!is_below_minimum_balance(2.0, 1.0));
    }

    #[test]
    fn parses_hours_minutes_seconds_days() {
        // ~400ms/slot is a reasonable Solana approximation for this test.
        assert_eq!(
            parse_sweep_interval_slots("1h", 400).unwrap(),
            3600 * 1000 / 400
        );
        assert_eq!(
            parse_sweep_interval_slots("30m", 400).unwrap(),
            30 * 60 * 1000 / 400
        );
        assert_eq!(
            parse_sweep_interval_slots("45s", 400).unwrap(),
            45 * 1000 / 400
        );
        assert_eq!(
            parse_sweep_interval_slots("1d", 400).unwrap(),
            86_400 * 1000 / 400
        );
    }

    #[test]
    fn rejects_malformed_or_unknown_unit_intervals() {
        assert!(matches!(
            parse_sweep_interval_slots("", 400),
            Err(SweepIntervalError::Empty)
        ));
        assert!(matches!(
            parse_sweep_interval_slots("1w", 400),
            Err(SweepIntervalError::UnknownUnit(_))
        ));
        assert!(matches!(
            parse_sweep_interval_slots("xh", 400),
            Err(SweepIntervalError::Malformed(_))
        ));
    }

    #[test]
    fn sweep_due_once_interval_has_elapsed() {
        assert!(!sweep_due(1000, 900, 200)); // only 100 slots elapsed
        assert!(sweep_due(1000, 800, 200)); // exactly 200 elapsed
        assert!(sweep_due(1000, 700, 200)); // well past
    }
}
