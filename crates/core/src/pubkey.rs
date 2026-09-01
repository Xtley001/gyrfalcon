//! Minimal 32-byte pubkey newtype.
//!
//! Stage A deliberately avoids depending on `solana-sdk` here: nothing in the
//! foundations stage needs an RPC client, a signer, or transaction types, and
//! that crate's dependency tree is heavy. `router`, `bundler`, and `submit`
//! will very likely want the real `solana_sdk::pubkey::Pubkey` (and
//! `VersionedTransaction`) once Stage B starts touching live program state —
//! swap this out then rather than carrying the weight now.

use std::fmt;

#[derive(
    Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Pubkey(pub [u8; 32]);

impl Pubkey {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn from_base58(s: &str) -> Result<Self, bs58::decode::Error> {
        let mut bytes = [0u8; 32];
        let written = bs58::decode(s).onto(&mut bytes)?;
        if written != 32 {
            return Err(bs58::decode::Error::BufferTooSmall);
        }
        Ok(Self(bytes))
    }

    pub fn to_base58(&self) -> String {
        bs58::encode(self.0).into_string()
    }
}

impl fmt::Debug for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pubkey({})", self.to_base58())
    }
}

impl fmt::Display for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_base58())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base58_round_trip() {
        let original = Pubkey::new([7u8; 32]);
        let encoded = original.to_base58();
        let decoded = Pubkey::from_base58(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn rejects_short_base58_strings() {
        let short = bs58::encode([1u8; 10]).into_string();
        let err = Pubkey::from_base58(&short);
        assert!(err.is_err());
    }
}
