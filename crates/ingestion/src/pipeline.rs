//! Wires an [`AccountSource`] through the [`DecoderRegistry`]'s owner
//! filter into the [`RingBuffer`], matching the ingestion stage of
//! `docs/ARCHITECTURE.md#system-overview`'s flow diagram
//! (`Geyser -> Ingestion -> ring buffer -> Health Engines`).
//!
//! This is intentionally the *only* place that turns a [`RawAccount`] into
//! a [`gyrfalcon_core::AccountUpdate`] — health adapters (Stage B+) consume
//! from the ring buffer, they never read a feed directly.

use crate::decode::{DecoderRegistry, RawAccount};
use crate::feed::AccountSource;
use crate::ring_buffer::{PushError, RingBuffer};
use gyrfalcon_core::AccountUpdate;

/// Drains every currently-available update from `source`, forwards the
/// ones whose owner is watched into `ring`, and returns how many were
/// forwarded vs. dropped (unwatched owner, or the ring buffer was full).
pub struct DrainStats {
    pub forwarded: usize,
    pub unwatched: usize,
    pub buffer_full: usize,
}

pub fn drain_into_ring_buffer(
    source: &mut dyn AccountSource,
    registry: &DecoderRegistry,
    ring: &RingBuffer,
) -> DrainStats {
    let mut stats = DrainStats {
        forwarded: 0,
        unwatched: 0,
        buffer_full: 0,
    };

    while let Some(raw) = source.next() {
        if !registry.is_watched(&raw.owner) {
            stats.unwatched += 1;
            continue;
        }
        let RawAccount {
            pubkey,
            owner,
            data,
            slot,
        } = raw;
        match ring.push(AccountUpdate {
            pubkey,
            owner,
            data,
            slot,
        }) {
            Ok(()) => stats.forwarded += 1,
            Err(PushError::Full) => stats.buffer_full += 1,
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::RecordedFeed;
    use gyrfalcon_core::Pubkey;

    // Recorded-batch fixture (synthetic, see feed.rs's module doc) proving
    // the full Stage A ingestion path: JSONL -> decode -> owner filter ->
    // ring buffer, ending in the correct in-memory AccountUpdate structs.
    fn fixture_jsonl(watched_owner: &Pubkey, unwatched_owner: &Pubkey) -> String {
        let pk = Pubkey::new([1u8; 32]).to_base58();
        let data_b64 = "3q2+7w=="; // 0xDE 0xAD 0xBE 0xEF
        format!(
            "{{\"pubkey\": \"{pk}\", \"owner\": \"{}\", \"data_base64\": \"{data_b64}\", \"slot\": 1}}\n\
             {{\"pubkey\": \"{pk}\", \"owner\": \"{}\", \"data_base64\": \"{data_b64}\", \"slot\": 2}}\n",
            watched_owner.to_base58(),
            unwatched_owner.to_base58(),
        )
    }

    #[test]
    fn recorded_batch_decodes_into_correct_ring_buffer_entries() {
        let watched = Pubkey::new([9u8; 32]);
        let unwatched = Pubkey::new([8u8; 32]);

        let jsonl = fixture_jsonl(&watched, &unwatched);
        let mut feed = RecordedFeed::parse(&jsonl).unwrap();

        let mut registry = DecoderRegistry::new();
        registry.watch(watched);

        let ring = RingBuffer::with_capacity(16);
        let stats = drain_into_ring_buffer(&mut feed, &registry, &ring);

        assert_eq!(stats.forwarded, 1);
        assert_eq!(stats.unwatched, 1);
        assert_eq!(stats.buffer_full, 0);

        let update = ring.pop().expect("watched-owner update should be queued");
        assert_eq!(update.slot, 1);
        assert_eq!(update.owner, watched);
        assert_eq!(update.data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(ring.pop().is_none());
    }

    #[test]
    fn full_ring_buffer_is_counted_not_silently_dropped() {
        let watched = Pubkey::new([9u8; 32]);
        let jsonl = fixture_jsonl(&watched, &watched); // both records watched
        let mut feed = RecordedFeed::parse(&jsonl).unwrap();

        let mut registry = DecoderRegistry::new();
        registry.watch(watched);

        let ring = RingBuffer::with_capacity(1); // room for exactly one
        let stats = drain_into_ring_buffer(&mut feed, &registry, &ring);

        assert_eq!(stats.forwarded, 1);
        assert_eq!(stats.buffer_full, 1);
    }
}
