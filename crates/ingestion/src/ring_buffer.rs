//! Shared-memory-style ring buffer between ingestion and the health
//! engines. Per `docs/ARCHITECTURE.md#ingestion`, decoded updates are
//! handed off through a lock-free ring buffer to keep the hot path off the
//! allocator.
//!
//! Backed by `crossbeam_queue::ArrayQueue`, a bounded lock-free MPMC ring
//! buffer — a hand-rolled unsafe SPSC ring is not worth the correctness
//! risk at this stage versus a well-audited crate that gives the same
//! allocation-free push/pop behavior on the hot path.

use gyrfalcon_core::AccountUpdate;
use std::sync::Arc;

pub struct RingBuffer {
    inner: Arc<crossbeam_queue::ArrayQueue<AccountUpdate>>,
}

/// Reason a push into the ring buffer did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushError {
    /// The buffer is full — the consumer (health engines) is falling
    /// behind. This is exactly the kind of condition
    /// `HealthAdapter::sync_lag_slots` (docs/API.md) exists to surface.
    Full,
}

impl RingBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(crossbeam_queue::ArrayQueue::new(capacity)),
        }
    }

    /// Cheap, `Send + Sync` handle sharing the same underlying buffer —
    /// used to hand a producer/consumer pair to different threads without
    /// wrapping the whole struct in a `Mutex`.
    pub fn handle(&self) -> RingBuffer {
        RingBuffer {
            inner: Arc::clone(&self.inner),
        }
    }

    pub fn push(&self, update: AccountUpdate) -> Result<(), PushError> {
        self.inner.push(update).map_err(|_| PushError::Full)
    }

    pub fn pop(&self) -> Option<AccountUpdate> {
        self.inner.pop()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::Pubkey;

    fn dummy_update(slot: u64) -> AccountUpdate {
        AccountUpdate {
            pubkey: Pubkey::new([1u8; 32]),
            owner: Pubkey::new([2u8; 32]),
            data: vec![0u8; 8],
            slot,
        }
    }

    #[test]
    fn push_then_pop_preserves_order() {
        let rb = RingBuffer::with_capacity(4);
        rb.push(dummy_update(1)).unwrap();
        rb.push(dummy_update(2)).unwrap();
        assert_eq!(rb.pop().unwrap().slot, 1);
        assert_eq!(rb.pop().unwrap().slot, 2);
        assert!(rb.pop().is_none());
    }

    #[test]
    fn full_buffer_returns_push_error_instead_of_blocking() {
        let rb = RingBuffer::with_capacity(1);
        rb.push(dummy_update(1)).unwrap();
        let err = rb.push(dummy_update(2)).unwrap_err();
        assert_eq!(err, PushError::Full);
    }

    #[test]
    fn handle_shares_the_same_underlying_buffer() {
        let rb = RingBuffer::with_capacity(4);
        let producer = rb.handle();
        producer.push(dummy_update(42)).unwrap();
        assert_eq!(rb.pop().unwrap().slot, 42);
    }
}
