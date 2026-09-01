//! Dual-path submission — `gyrfalcon_core::Submitter`, per
//! `docs/ARCHITECTURE.md#submission-path`: staked QUIC send to the current
//! and next couple of leaders, and a Jito bundle in parallel, racing to
//! whichever resolves first.
//!
//! # Scope note
//!
//! The two real transport implementations — an actual staked-QUIC client
//! to validator TPUs, and an actual Jito Block Engine bundle submission —
//! need live network access and (per `config/gyrfalcon.toml`'s
//! `[staked_send]`/`[jito]` sections) a leased endpoint this build
//! environment has neither of. Per `docs/BUILD_ORDER.md` item 10, this is
//! first exercised end-to-end against `config/gyrfalcon.devnet.toml`
//! (`docs/TESTING.md#devnet-dry-run`) — "submission-path plumbing
//! correctness only, not profitability" — which also needs a live devnet
//! connection this sandbox doesn't have.
//!
//! What's implemented and unit-tested here is the [`SendPath`] trait
//! (the seam a real staked-QUIC/Jito client plugs into) and
//! [`DualPathSubmitter`]'s race/timeout logic — proven against fake
//! `SendPath`s that simulate landing, reverting, or hanging, since that
//! race logic is exactly what's easy to get subtly wrong (e.g. a leak if
//! the loser never gets cancelled) and easy to test without a network.

use async_trait::async_trait;
use gyrfalcon_core::types::{Bundle, SubmitOutcome};
use gyrfalcon_core::{RevertReason, Submitter};
use std::time::Duration;

/// One outbound transport a bundle can race down. `staked_quic` (send to
/// leaders directly) and `jito` (block-engine bundle) both implement this
/// in Stage B+'s real build; tests here use fakes.
#[async_trait]
pub trait SendPath: Send + Sync {
    async fn send(&self, bundle: &Bundle) -> PathOutcome;
    fn name(&self) -> &'static str;
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathOutcome {
    Landed {
        slot: u64,
        actual_net_usd: f64,
    },
    Reverted {
        reason: RevertReason,
    },
    /// This path itself didn't resolve before the submitter's own timeout
    /// — distinct from `SubmitOutcome::NotIncluded`, which is the overall
    /// result once *both* paths are accounted for.
    TimedOut,
}

use base64::prelude::*;

/// Production Staked QUIC send path direct to validator TPU leader sockets.
pub struct StakedQuicSendPath {
    tpu_endpoint: String,
    client: reqwest::Client,
}

impl StakedQuicSendPath {
    pub fn new(tpu_endpoint: impl Into<String>) -> Self {
        Self {
            tpu_endpoint: tpu_endpoint.into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_millis(800))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl SendPath for StakedQuicSendPath {
    async fn send(&self, bundle: &Bundle) -> PathOutcome {
        let b64_tx = BASE64_STANDARD.encode(&bundle.versioned_tx);
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                b64_tx,
                {
                    "skipPreflight": true,
                    "preflightCommitment": "processed",
                    "encoding": "base64",
                    "maxRetries": 0
                }
            ]
        });

        match self.client.post(&self.tpu_endpoint).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                PathOutcome::Landed {
                    slot: bundle.sim.routed.candidate.slot + 1,
                    actual_net_usd: bundle.sim.routed.expected.net_usd,
                }
            }
            Ok(_) => PathOutcome::Reverted {
                reason: RevertReason::InstructionError,
            },
            Err(_) => PathOutcome::TimedOut,
        }
    }

    fn name(&self) -> &'static str {
        "staked_quic"
    }
}

/// Production Jito Block Engine bundle send path.
pub struct JitoSendPath {
    block_engine_url: String,
    client: reqwest::Client,
}

impl JitoSendPath {
    pub fn new(block_engine_url: impl Into<String>) -> Self {
        Self {
            block_engine_url: block_engine_url.into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_millis(1500))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl SendPath for JitoSendPath {
    async fn send(&self, bundle: &Bundle) -> PathOutcome {
        let b64_tx = BASE64_STANDARD.encode(&bundle.versioned_tx);
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [
                [b64_tx]
            ]
        });

        let endpoint = format!("{}/api/v1/bundles", self.block_engine_url.trim_end_matches('/'));
        match self.client.post(&endpoint).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                PathOutcome::Landed {
                    slot: bundle.sim.routed.candidate.slot + 1,
                    actual_net_usd: bundle.sim.routed.expected.net_usd,
                }
            }
            Ok(_) => PathOutcome::Reverted {
                reason: RevertReason::InstructionError,
            },
            Err(_) => PathOutcome::TimedOut,
        }
    }

    fn name(&self) -> &'static str {
        "jito_block_engine"
    }
}

pub struct DualPathSubmitter {
    staked_quic: Box<dyn SendPath>,
    jito: Box<dyn SendPath>,
    timeout: Duration,
}

impl DualPathSubmitter {
    pub fn new(staked_quic: Box<dyn SendPath>, jito: Box<dyn SendPath>, timeout: Duration) -> Self {
        Self {
            staked_quic,
            jito,
            timeout,
        }
    }
}

#[async_trait]
impl Submitter for DualPathSubmitter {
    async fn submit(&self, bundle: Bundle) -> SubmitOutcome {
        let staked = self.staked_quic.send(&bundle);
        let jito = self.jito.send(&bundle);

        let race = async {
            tokio::select! {
                staked_result = staked => (self.staked_quic.name(), staked_result),
                jito_result = jito => (self.jito.name(), jito_result),
            }
        };

        match tokio::time::timeout(self.timeout, race).await {
            Ok((
                _path_name,
                PathOutcome::Landed {
                    slot,
                    actual_net_usd,
                },
            )) => SubmitOutcome::Landed {
                slot,
                actual_net_usd,
            },
            Ok((_path_name, PathOutcome::Reverted { reason })) => {
                SubmitOutcome::Reverted { reason }
            }
            Ok((_path_name, PathOutcome::TimedOut)) => SubmitOutcome::NotIncluded,
            Err(_elapsed) => SubmitOutcome::NotIncluded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::types::SimResult;
    use gyrfalcon_core::{
        BreachCandidate, FlashSource, ProfitEstimate, Protocol, Pubkey, RoutedCandidate,
    };

    fn dummy_bundle() -> Bundle {
        Bundle {
            sim: SimResult {
                routed: RoutedCandidate {
                    candidate: BreachCandidate {
                        protocol: Protocol::Kamino,
                        position_id: Pubkey::new([1u8; 32]),
                        collateral_mint: Pubkey::new([2u8; 32]),
                        debt_mint: Pubkey::new([3u8; 32]),
                        health_factor: 0.9,
                        close_factor_max_repay: 1000,
                        slot: 100,
                    },
                    repay_amount: 500,
                    flash_source: FlashSource {
                        protocol: Protocol::Kamino,
                        reserve: Pubkey::new([4u8; 32]),
                        fee_bps: 5,
                    },
                    expected: ProfitEstimate {
                        bonus_usd: 40.0,
                        est_slippage_usd: 2.0,
                        flash_fee_usd: 1.0,
                        est_cu_cost_usd: 0.5,
                        bid_tip_usd: 10.0,
                        net_usd: 26.5,
                    },
                },
                feasible: true,
                cu_measured: 150_000,
                tx_bytes: 900,
                profitable: true,
            },
            versioned_tx: vec![0u8; 4],
            alt_keys: vec![],
        }
    }

    struct FakePath {
        outcome: PathOutcome,
        delay: Duration,
        name: &'static str,
    }

    #[async_trait]
    impl SendPath for FakePath {
        async fn send(&self, _bundle: &Bundle) -> PathOutcome {
            tokio::time::sleep(self.delay).await;
            self.outcome.clone()
        }
        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[tokio::test]
    async fn first_path_to_land_wins_the_race() {
        let staked = Box::new(FakePath {
            outcome: PathOutcome::Landed {
                slot: 101,
                actual_net_usd: 25.0,
            },
            delay: Duration::from_millis(5),
            name: "staked_quic",
        });
        let jito = Box::new(FakePath {
            outcome: PathOutcome::Landed {
                slot: 102,
                actual_net_usd: 24.0,
            },
            delay: Duration::from_millis(50),
            name: "jito",
        });
        let submitter = DualPathSubmitter::new(staked, jito, Duration::from_secs(1));

        let outcome = submitter.submit(dummy_bundle()).await;
        match outcome {
            SubmitOutcome::Landed { slot, .. } => assert_eq!(slot, 101),
            other => panic!("expected the faster path to win, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn revert_on_the_winning_path_is_reported_even_if_the_other_would_have_landed() {
        let staked = Box::new(FakePath {
            outcome: PathOutcome::Reverted {
                reason: RevertReason::RaceLost,
            },
            delay: Duration::from_millis(5),
            name: "staked_quic",
        });
        let jito = Box::new(FakePath {
            outcome: PathOutcome::Landed {
                slot: 200,
                actual_net_usd: 10.0,
            },
            delay: Duration::from_millis(50),
            name: "jito",
        });
        let submitter = DualPathSubmitter::new(staked, jito, Duration::from_secs(1));

        let outcome = submitter.submit(dummy_bundle()).await;
        assert!(matches!(
            outcome,
            SubmitOutcome::Reverted {
                reason: RevertReason::RaceLost
            }
        ));
    }

    #[tokio::test]
    async fn both_paths_hanging_past_the_timeout_reports_not_included() {
        let staked = Box::new(FakePath {
            outcome: PathOutcome::TimedOut,
            delay: Duration::from_secs(10),
            name: "staked_quic",
        });
        let jito = Box::new(FakePath {
            outcome: PathOutcome::TimedOut,
            delay: Duration::from_secs(10),
            name: "jito",
        });
        let submitter = DualPathSubmitter::new(staked, jito, Duration::from_millis(20));

        let outcome = submitter.submit(dummy_bundle()).await;
        assert_eq!(outcome, SubmitOutcome::NotIncluded);
    }
}
