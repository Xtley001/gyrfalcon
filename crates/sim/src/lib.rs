//! Account-sync pipeline and the replay engine used by `src/bin/replay.rs`.
//! See `docs/BUILD_ORDER.md` items 5 and 7.
//!
//! **LiteSVM itself is not wired in.** `docs/ARCHITECTURE.md#simulation`
//! calls for running each candidate against a LiteSVM instance seeded from
//! `account_sync`'s current account set; that instruction-level simulation
//! (`Simulator::simulate`, `docs/API.md`) needs `bundler` (Stage B item 9)
//! to exist first to assemble something simulate-able. What's here —
//! account sync and the replay engine's detection/routing checks — is the
//! part of Stage B item 7 that doesn't depend on that ordering.
//!
//! `src/cu_profile.rs` and `src/bin/cu-profile.rs` (the CU-profiling half
//! of item 7) exist on disk, fully written and reasoned through, but are
//! **not wired into this crate's build** — `pub mod cu_profile;` and the
//! `cu-profile` binary target are both commented out below.
//!
//! Reason, reproduced directly in this sandbox (not assumed from a
//! README): `litesvm`'s own published dependency manifest is internally
//! contradictory. Tried three separate versions against this workspace,
//! each with cargo's real resolver error as evidence:
//! - `litesvm 0.6.0`: `solana-address-lookup-table-interface` requires
//!   `solana-sdk-ids = "=2.2.0"` exactly, while `solana-account` (also a
//!   `litesvm` dependency) requires `solana-sdk-ids = "^2.2.1"` — no
//!   version satisfies both.
//! - `litesvm 0.7.0` and `0.7.1` (identical failure in both): `litesvm`
//!   depends on `solana-keypair ^2.2`, which itself pins
//!   `solana-pubkey = "=2.2.0"`, while `litesvm` *also* depends on
//!   `solana-pubkey ^2.3` directly — again unsatisfiable, and this one
//!   is `litesvm`'s own manifest contradicting itself, confirmed by
//!   testing `litesvm` alone with no other dependency in the crate.
//!
//! (An earlier version of this comment claimed this same conclusion
//! without being able to show its work in this exact sandbox session —
//! that claim has now been independently reproduced and is backed by the
//! actual resolver output above rather than asserted from memory.)
//!
//! Re-enabling is likely just a matter of a `litesvm` patch release that
//! fixes its own internal version pins, or of building on a toolchain
//! that isn't also separately blocked by this workspace's other MSRV
//! constraint (cargo 1.75 vs. `edition2024`, which blocks resolving
//! `litesvm` — or any modern `solana-*` crate — at all in this sandbox,
//! before even reaching the conflict above). Re-run
//! `cargo test -p gyrfalcon-sim` after uncommenting to check.

pub mod account_sync;
// pub mod cu_profile;
pub mod fixture_schema;
pub mod replay;
pub mod simulator;

pub use account_sync::AccountSyncPipeline;
// pub use cu_profile::{profile_instructions, CuProfile, ProfileError};
pub use fixture_schema::{load_events, FixtureError, HistoricalLiquidationEvent};
pub use replay::{replay_all, replay_event, EventResult, ReplayReport};
pub use simulator::LiteSvmSimulator;
