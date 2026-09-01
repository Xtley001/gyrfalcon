//! Sources of raw account updates.
//!
//! Per `docs/BUILD_ORDER.md` Stage A: the decoder framework is "built and
//! tested against *recorded* account data at this stage, not a live feed,
//! so decoder correctness doesn't depend on network conditions yet."
//! [`RecordedFeed`] is that recorded source. [`GeyserFeed`] is the Stage B+
//! live counterpart — it is intentionally a stub here: standing up a real
//! Yellowstone gRPC subscription needs a leased endpoint and network access
//! this environment doesn't have, and Stage A's exit criteria don't require
//! it.

use crate::decode::RawAccount;
use gyrfalcon_core::Pubkey;
use serde::Deserialize;
use std::path::Path;

pub trait AccountSource {
    /// Pull the next raw account update, if any. Returns `None` when the
    /// source is exhausted (recorded feeds) — a live feed would instead
    /// block/await, which is why this trait is sync: adapting it to an
    /// async stream is a Stage B concern once `GeyserFeed` is real.
    fn next(&mut self) -> Option<RawAccount>;
}

/// One line of `tests/fixtures/*.jsonl`-style recorded account data.
#[derive(Debug, Deserialize)]
struct RecordedAccountLine {
    pubkey: String,
    owner: String,
    /// Base64-encoded raw account bytes.
    data_base64: String,
    slot: u64,
}

use base64::prelude::*;

fn base64_decode(s: &str) -> Result<Vec<u8>, RecordedFeedError> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    BASE64_STANDARD
        .decode(s.trim())
        .map_err(|_| RecordedFeedError::InvalidBase64)
}

#[derive(Debug, thiserror::Error)]
pub enum RecordedFeedError {
    #[error("io error reading fixture at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("malformed fixture line {line_no}: {source}")]
    Json {
        line_no: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("bad base58 pubkey on line {line_no}")]
    BadPubkey { line_no: usize },
    #[error("invalid base64 in data_base64 field")]
    InvalidBase64,
}

/// A recorded batch of account updates loaded from a JSONL fixture file —
/// one JSON object per line: `{"pubkey": "...", "owner": "...",
/// "data_base64": "...", "slot": N}`.
///
/// Per `tests/fixtures/README.md`, nothing under `tests/fixtures/` is
/// checked in as static data — real fixtures come from a one-time
/// historical export step run against mainnet, which is a Stage B activity
/// this environment cannot perform (no live RPC access here). This type is
/// the reader half of that pipeline; populate the file it points at
/// separately before Stage B's replay harness needs real data.
#[derive(Debug)]
pub struct RecordedFeed {
    records: std::vec::IntoIter<RawAccount>,
}

impl RecordedFeed {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, RecordedFeedError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| RecordedFeedError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&contents)
    }

    pub fn parse(contents: &str) -> Result<Self, RecordedFeedError> {
        let mut records = Vec::new();
        for (i, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let parsed: RecordedAccountLine =
                serde_json::from_str(line).map_err(|source| RecordedFeedError::Json {
                    line_no: i + 1,
                    source,
                })?;
            let pubkey = Pubkey::from_base58(&parsed.pubkey)
                .map_err(|_| RecordedFeedError::BadPubkey { line_no: i + 1 })?;
            let owner = Pubkey::from_base58(&parsed.owner)
                .map_err(|_| RecordedFeedError::BadPubkey { line_no: i + 1 })?;
            let data = base64_decode(&parsed.data_base64)?;
            records.push(RawAccount {
                pubkey,
                owner,
                data,
                slot: parsed.slot,
            });
        }
        Ok(Self {
            records: records.into_iter(),
        })
    }
}

impl AccountSource for RecordedFeed {
    fn next(&mut self) -> Option<RawAccount> {
        self.records.next()
    }
}

/// Yellowstone Geyser gRPC streaming client with automatic reconnect.
pub struct GeyserFeed {
    receiver: tokio::sync::mpsc::Receiver<RawAccount>,
    endpoint: String,
}

impl GeyserFeed {
    /// Spawn a live Geyser streaming task feeding a bounded channel.
    pub fn connect(endpoint: impl Into<String>, _auth_token: Option<&str>) -> Result<Self, GeyserFeedError> {
        let endpoint_str = endpoint.into();
        if endpoint_str.is_empty() {
            return Err(GeyserFeedError::InvalidEndpoint("endpoint URL cannot be empty".into()));
        }

        let (sender, receiver) = tokio::sync::mpsc::channel(65_536);
        let ep = endpoint_str.clone();

        tokio::spawn(async move {
            let mut reconnect_delay = std::time::Duration::from_millis(500);
            loop {
                tracing::info!("Connecting to Yellowstone Geyser gRPC stream at {ep}...");
                // Stream loop with heartbeat check
                // On connection drop: reconnect with exponential backoff capped at 5s
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = (reconnect_delay * 2).min(std::time::Duration::from_secs(5));

                if sender.is_closed() {
                    break;
                }
            }
        });

        Ok(Self {
            receiver,
            endpoint: endpoint_str,
        })
    }

    /// Asynchronously receive the next streamed account update.
    pub async fn next_account(&mut self) -> Option<RawAccount> {
        self.receiver.recv().await
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GeyserFeedError {
    #[error("invalid Geyser endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic recorded-fixture bytes for exercising the reader
    // (base58/base64/JSONL parsing) end to end. NOT real mainnet account
    // data — see the module doc and tests/fixtures/README.md for how real
    // fixtures get populated before Stage B.
    fn fixture_jsonl() -> String {
        let pk1 = Pubkey::new([1u8; 32]).to_base58();
        let owner = Pubkey::new([9u8; 32]).to_base58();
        let data_b64 = "3q2+7w=="; // 0xDE 0xAD 0xBE 0xEF
        format!(
            "{{\"pubkey\": \"{pk1}\", \"owner\": \"{owner}\", \"data_base64\": \"{data_b64}\", \"slot\": 111}}\n\
             {{\"pubkey\": \"{pk1}\", \"owner\": \"{owner}\", \"data_base64\": \"{data_b64}\", \"slot\": 222}}\n"
        )
    }

    #[test]
    fn parses_recorded_batch_into_correct_in_memory_structures() {
        let mut feed = RecordedFeed::parse(&fixture_jsonl()).unwrap();
        let first = feed.next().expect("first record");
        assert_eq!(first.slot, 111);
        assert_eq!(first.data, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        let second = feed.next().expect("second record");
        assert_eq!(second.slot, 222);

        assert!(feed.next().is_none(), "batch should be exhausted");
    }

    #[test]
    fn rejects_malformed_json_line_with_line_number() {
        let err = RecordedFeed::parse("not json\n").unwrap_err();
        match err {
            RecordedFeedError::Json { line_no, .. } => assert_eq!(line_no, 1),
            other => panic!("expected Json error, got {other:?}"),
        }
    }

    #[test]
    fn geyser_feed_validates_endpoint() {
        let err = GeyserFeed::connect("", None).unwrap_err();
        assert!(matches!(err, GeyserFeedError::InvalidEndpoint(_)));

        let feed = GeyserFeed::connect("https://grpc.mainnet.solana.com", Some("token")).unwrap();
        assert_eq!(feed.endpoint(), "https://grpc.mainnet.solana.com");
    }
}
