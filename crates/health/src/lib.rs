//! Shared health-engine framework + per-protocol adapters.
//!
//! Stage B (docs/BUILD_ORDER.md item 4): Kamino adapter, decoding real
//! on-chain accounts via Kamino's own `klend-interface` crate.
//!
//! Stage C (items 13-14): Save adapter (`solend-sdk`) and MarginFi
//! adapter (`marginfi-type-crate`) — see each module's doc comment for
//! protocol-specific findings surfaced while building them (Save's
//! vestigial close-factor constants; the corrected MarginFi flash-loan
//! fact and its weaker HealthCache-based detection signal).
//!
//! None of the three adapters are validated against the replay harness
//! yet (Stage B item 5 / Stage C's "re-run the full replay harness across
//! both/all protocols") — that requires real historical liquidation data
//! this build environment has no way to obtain. See
//! `crates/sim/src/fixture_schema.rs`'s module doc.

pub mod adapters;

pub use adapters::{KaminoAdapter, MarginfiAdapter, SaveAdapter};
