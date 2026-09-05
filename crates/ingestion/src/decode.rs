//! Generic zero-copy account decoder framework.
//!
//! Per `docs/ARCHITECTURE.md#ingestion`: "raw Borsh structs generated
//! directly from each protocol's IDL... zero-copy casts wherever the
//! account layout allows fixed-size, `#[repr(C)]`-safe structs." This
//! module is the *generic* framework only — no protocol-specific layouts
//! live here. Per `docs/BUILD_ORDER.md` Stage A, protocol decoders (Kamino
//! first, in Stage B) are pinned to each program's live IDL, not written
//! against a cached/assumed layout, and are out of scope until then.
//!
//! A stale layout produces a silently wrong health-factor number rather
//! than a visible crash (ARCHITECTURE.md), so every decoder here returns
//! `None` on a length/alignment mismatch instead of panicking or truncating.

use gyrfalcon_core::Pubkey;

/// A account layout that can be reinterpreted from raw bytes with no copy,
/// no allocation, and no parsing — `bytemuck`'s `Pod` bound is what makes
/// this safe: fixed layout, no padding ambiguity, no invalid bit patterns.
pub trait ZeroCopyAccount: bytemuck::Pod {
    /// The 8-byte (or protocol-specific) discriminator this layout expects
    /// at the front of the account, if the owning program uses one. `None`
    /// means the whole buffer is the struct with no discriminator prefix.
    const DISCRIMINATOR: Option<&'static [u8]> = None;

    /// Zero-copy-*style* decode: returns an owned copy via an unaligned
    /// read rather than a reference. Account bytes arrive as an
    /// arbitrarily-offset slice (e.g. right after a discriminator prefix),
    /// so a reference cast via `bytemuck::try_from_bytes` would reject
    /// perfectly valid data on any layout with `u64`+ fields whenever that
    /// offset breaks natural alignment. `try_pod_read_unaligned` avoids a
    /// full parse/branch tree while still being a single non-allocating
    /// memcpy, which is what "zero-copy" is buying here in practice: no
    /// parser, not literally zero bytes moved.
    ///
    /// Returns `None` on any length/discriminator mismatch — the caller
    /// (an adapter, from Stage B onward) is responsible for deciding
    /// whether a decode failure is expected (wrong account type) or a
    /// signal that the pinned layout is stale against a program upgrade.
    fn decode(data: &[u8]) -> Option<Self> {
        let body = match Self::DISCRIMINATOR {
            Some(disc) => {
                if data.len() < disc.len() || &data[..disc.len()] != disc {
                    return None;
                }
                &data[disc.len()..]
            }
            None => data,
        };
        bytemuck::try_pod_read_unaligned(body).ok()
    }
}

/// One raw account exactly as it arrived off the wire (or off a recorded
/// fixture) — undecoded. This is what [`crate::feed::AccountSource`]
/// produces and what the decoder registry below consumes.
#[derive(Debug, Clone)]
pub struct RawAccount {
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub data: Vec<u8>,
    pub slot: u64,
}

/// Maps a program (owner) pubkey to the decoder that knows how to read its
/// accounts, and turns a stream of [`RawAccount`]s into [`gyrfalcon_core::AccountUpdate`]s
/// for the ring buffer. Registration happens once at adapter construction
/// time in Stage B (e.g. the Kamino adapter registers its reserve and
/// obligation layouts); Stage A only proves the plumbing works.
use std::collections::HashSet;

pub struct DecoderRegistry {
    owners: HashSet<Pubkey>,
}

impl Default for DecoderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DecoderRegistry {
    pub fn new() -> Self {
        Self {
            owners: HashSet::new(),
        }
    }

    /// Register a program whose accounts should be forwarded downstream.
    pub fn watch(&mut self, owner: Pubkey) {
        self.owners.insert(owner);
    }

    #[inline(always)]
    pub fn is_watched(&self, owner: &Pubkey) -> bool {
        self.owners.contains(owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::{Pod, Zeroable};

    // A generic fixed-layout struct used ONLY to prove the decode
    // mechanics (alignment, discriminator handling, length checks). This
    // is NOT a Kamino account layout — those are pinned to
    // each program's live IDL in Stage B, per docs/BUILD_ORDER.md, and must
    // not be inferred or guessed here.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
    struct DemoReserveLayout {
        pub mint: [u8; 32],
        pub available_liquidity: u64,
        pub borrowed_liquidity: u64,
    }

    impl ZeroCopyAccount for DemoReserveLayout {
        const DISCRIMINATOR: Option<&'static [u8]> = Some(&[0xDE, 0xAD, 0xBE, 0xEF]);
    }

    fn encode_demo(mint: [u8; 32], available: u64, borrowed: u64) -> Vec<u8> {
        let layout = DemoReserveLayout {
            mint,
            available_liquidity: available,
            borrowed_liquidity: borrowed,
        };
        let mut out = vec![0xDE, 0xAD, 0xBE, 0xEF];
        out.extend_from_slice(bytemuck::bytes_of(&layout));
        out
    }

    #[test]
    fn decodes_well_formed_account() {
        let bytes = encode_demo([7u8; 32], 1_000_000, 250_000);
        let decoded = DemoReserveLayout::decode(&bytes).expect("should decode");
        assert_eq!(decoded.mint, [7u8; 32]);
        assert_eq!(decoded.available_liquidity, 1_000_000);
        assert_eq!(decoded.borrowed_liquidity, 250_000);
    }

    #[test]
    fn rejects_wrong_discriminator_instead_of_misreading() {
        let mut bytes = encode_demo([1u8; 32], 1, 2);
        bytes[0] = 0x00; // corrupt the discriminator
        assert!(DemoReserveLayout::decode(&bytes).is_none());
    }

    #[test]
    fn rejects_truncated_account_instead_of_panicking() {
        let bytes = encode_demo([1u8; 32], 1, 2);
        let truncated = &bytes[..bytes.len() - 3];
        assert!(DemoReserveLayout::decode(truncated).is_none());
    }

    #[test]
    fn registry_tracks_watched_owners() {
        let mut reg = DecoderRegistry::new();
        let kamino = Pubkey::new([1u8; 32]);
        let unrelated = Pubkey::new([2u8; 32]);
        reg.watch(kamino);
        assert!(reg.is_watched(&kamino));
        assert!(!reg.is_watched(&unrelated));
    }
}
