//! Enums shared across every crate.

/// The three lending markets `gyrfalcon` covers.
///
/// Build order (docs/BUILD_ORDER.md): Kamino ships alone in Stage B, then
/// Save and MarginFi are added in Stage C. All three variants exist from
/// Stage A so downstream types compile, even though only `Kamino` has a
/// real adapter until Stage C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Protocol {
    Kamino,
    Save,
    MarginFi,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Protocol::Kamino => "kamino",
            Protocol::Save => "save",
            Protocol::MarginFi => "marginfi",
        };
        write!(f, "{s}")
    }
}

/// Why a submitted transaction reverted, per `SubmitOutcome::Reverted`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RevertReason {
    /// Someone else liquidated the position first.
    RaceLost,
    /// Simulation was stale relative to the landed slot.
    SlippageExceeded,
    /// The route's compute-unit estimate undershot actual execution.
    ComputeExhausted,
    /// Flash-loan repayment failed within the same transaction.
    FlashRepayFailed,
    /// Catch-all for a program error not otherwise classified; the raw
    /// program log is preserved for later triage.
    ProgramError(String),
}
