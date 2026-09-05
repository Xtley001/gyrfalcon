//! Enums shared across every crate.
//!
//! Protocol enum decision locked in Step 0 of docs/07_BUILD_ORDER.md:
//! Single-variant Kamino only; Save and MarginFi variants removed per migration directive.

/// The single lending market `gyrfalcon` covers (Kamino Lend only).
///
/// Migration rework (docs/07_BUILD_ORDER.md Step 0): Protocol retains only
/// `Kamino`. Save and MarginFi are removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Protocol {
    Kamino,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Protocol::Kamino => "kamino",
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
