//! Shared data types and trait contracts for `gyrfalcon`.
//!
//! This is the crate every other crate in the workspace depends on. It has
//! no business logic of its own — see `docs/API.md`, which this module
//! mirrors closely enough that a diff between the two should be rare and
//! deliberate.

pub mod protocol;
pub mod pubkey;
pub mod traits;
pub mod types;

pub use protocol::{Protocol, RevertReason};
pub use pubkey::Pubkey;
pub use traits::{AccountUpdate, FlashSourceRouter, HealthAdapter, Simulator, Submitter};
pub use types::{
    BreachCandidate, Bundle, FlashSource, LiquidationRecord, ProfitEstimate, RoutedCandidate,
    SimResult, SubmitOutcome,
};
