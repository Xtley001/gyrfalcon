//! Per-protocol `HealthAdapter` implementations.
//!
//! Build order (docs/BUILD_ORDER.md): `kamino` in Stage B (done); `save`
//! in Stage C item 13 (done — see `save.rs`'s module doc for a real
//! finding about its close-factor constants being vestigial); `marginfi`
//! in Stage C item 14 (done — see `marginfi.rs`'s module doc for the
//! corrected flash-loan finding: MarginFi *does* have a native flash
//! instruction, contrary to `docs/ARCHITECTURE.md`'s assumption).
//!
//! All three share the MSRV 1.81 caveat noted in `kamino.rs`'s module doc
//! — none of them could be built/tested in the sandbox that wrote them
//! (cargo 1.75).

pub mod kamino;
pub mod marginfi;
pub mod save;

pub use kamino::KaminoAdapter;
pub use marginfi::MarginfiAdapter;
pub use save::SaveAdapter;
