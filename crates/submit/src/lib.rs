//! Staked QUIC + Jito dual send. See `docs/ARCHITECTURE.md#submission-path`
//! and `dual_path.rs`'s module doc for exactly what is and isn't wired to
//! a real network in this build.

pub mod dual_path;

pub use dual_path::{DualPathSubmitter, JitoSendPath, PathOutcome, SendPath, StakedQuicSendPath};
