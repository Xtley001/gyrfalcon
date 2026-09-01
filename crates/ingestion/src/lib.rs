//! Geyser ingestion: account decoding and the ring-buffer hand-off to the
//! health engines. See `docs/ARCHITECTURE.md#ingestion`.
//!
//! Stage A scope (docs/BUILD_ORDER.md): the generic zero-copy decoder
//! framework and the ring buffer, proven against recorded fixture data.
//! Protocol-specific layouts and the live Geyser subscription are Stage B+.

pub mod decode;
pub mod feed;
pub mod pipeline;
pub mod ring_buffer;

pub use decode::{DecoderRegistry, RawAccount, ZeroCopyAccount};
pub use feed::{AccountSource, GeyserFeed, RecordedFeed};
pub use pipeline::{drain_into_ring_buffer, DrainStats};
pub use ring_buffer::{PushError, RingBuffer};
