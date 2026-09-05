//! Per-protocol `HealthAdapter` implementations.
//!
//! Gyrfalcon targets Kamino Lend exclusively per `01_PROTOCOLS.md`.

pub mod kamino;

pub use kamino::KaminoAdapter;
