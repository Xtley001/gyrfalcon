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
                reason: RevertReason::ProgramError("transaction failed on-chain".to_string()),
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
                reason: RevertReason::ProgramError("transaction failed on-chain".to_string()),
            },
            Err(_) => PathOutcome::TimedOut,
        }
    }

    fn name(&self) -> &'static str {
        "jito_block_engine"
    }
}

/// 8 verified official Jito tip accounts on Solana Mainnet-Beta per
/// `03_SUBMISSION_LATENCY.md §2`.
pub const DEFAULT_JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

/// Jito tip account manager handling dynamic polling via `getTipAccounts`
/// with hard fallback to the 8 verified accounts on failure.
pub struct JitoTipManager {
    block_engine_url: String,
    active_tip_accounts: std::sync::RwLock<Vec<String>>,
    client: reqwest::Client,
}

impl JitoTipManager {
    pub fn new(block_engine_url: impl Into<String>) -> Self {
        Self {
            block_engine_url: block_engine_url.into(),
            active_tip_accounts: std::sync::RwLock::new(
                DEFAULT_JITO_TIP_ACCOUNTS.iter().map(|s| s.to_string()).collect(),
            ),
            client: reqwest::Client::builder()
                .timeout(Duration::from_millis(1500))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Retrieve the currently active tip accounts.
    pub fn get_active_tip_accounts(&self) -> Vec<String> {
        self.active_tip_accounts
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| DEFAULT_JITO_TIP_ACCOUNTS.iter().map(|s| s.to_string()).collect())
    }

    /// Select one tip account pseudo-randomly to spread load per Jito guidance.
    pub fn random_tip_account(&self) -> String {
        let accounts = self.get_active_tip_accounts();
        if accounts.is_empty() {
            return DEFAULT_JITO_TIP_ACCOUNTS[0].to_string();
        }
        let idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as usize)
            .unwrap_or(0))
            % accounts.len();
        accounts[idx].clone()
    }

    /// Overwrite the active tip accounts directly (for testing or overrides).
    pub fn update_accounts(&self, accounts: Vec<String>) {
        if let Ok(mut guard) = self.active_tip_accounts.write() {
            *guard = accounts;
        }
    }

    /// Call `getTipAccounts` on the Jito Block Engine endpoint.
    /// Updates the active list on success; falls back to `DEFAULT_JITO_TIP_ACCOUNTS` on failure.
    pub async fn refresh_tip_accounts(&self) -> Result<Vec<String>, String> {
        let endpoint = format!("{}/api/v1/bundles", self.block_engine_url.trim_end_matches('/'));
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTipAccounts",
            "params": []
        });

        match self.client.post(&endpoint).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = body.get("result").and_then(|r| r.as_array()) {
                        let parsed: Vec<String> = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        if !parsed.is_empty() {
                            self.update_accounts(parsed.clone());
                            tracing::info!(count = parsed.len(), "Refreshed Jito tip accounts from live endpoint");
                            return Ok(parsed);
                        }
                    }
                }
                self.fallback_to_defaults("invalid or empty JSON-RPC response from getTipAccounts")
            }
            Ok(resp) => self.fallback_to_defaults(&format!("HTTP error {}", resp.status())),
            Err(e) => self.fallback_to_defaults(&format!("network error: {e}")),
        }
    }

    fn fallback_to_defaults(&self, reason: &str) -> Result<Vec<String>, String> {
        let fallback: Vec<String> = DEFAULT_JITO_TIP_ACCOUNTS.iter().map(|s| s.to_string()).collect();
        self.update_accounts(fallback.clone());
        tracing::warn!(reason = reason, "Failed to refresh Jito tip accounts, falling back to 8 verified defaults");
        Err(format!("Fallback to defaults: {reason}"))
    }
}

/// Provider trait for leader-schedule awareness.
pub trait LeaderScheduleProvider: Send + Sync {
    /// Check whether any of the upcoming slots in the window [current_slot, current_slot + leaders_ahead]
    /// are scheduled to be led by a Jito-Solana validator.
    fn has_jito_leader_in_window(&self, current_slot: u64, leaders_ahead: u32) -> bool;
}

/// Static leader-schedule implementation for testing and offline simulation.
pub struct StaticLeaderSchedule {
    jito_slots: std::collections::HashSet<u64>,
}

impl StaticLeaderSchedule {
    pub fn new(jito_slots: impl IntoIterator<Item = u64>) -> Self {
        Self {
            jito_slots: jito_slots.into_iter().collect(),
        }
    }
}

impl LeaderScheduleProvider for StaticLeaderSchedule {
    fn has_jito_leader_in_window(&self, current_slot: u64, leaders_ahead: u32) -> bool {
        for slot in current_slot..=current_slot + (leaders_ahead as u64) {
            if self.jito_slots.contains(&slot) {
                return true;
            }
        }
        false
    }
}

pub struct DualPathSubmitter {
    staked_quic: Box<dyn SendPath>,
    jito: Box<dyn SendPath>,
    timeout: Duration,
    leaders_ahead: u32,
    leader_schedule: Option<Box<dyn LeaderScheduleProvider>>,
}

impl DualPathSubmitter {
    pub fn new(staked_quic: Box<dyn SendPath>, jito: Box<dyn SendPath>, timeout: Duration) -> Self {
        Self {
            staked_quic,
            jito,
            timeout,
            leaders_ahead: 2,
            leader_schedule: None,
        }
    }

    pub fn with_leader_schedule(
        mut self,
        leaders_ahead: u32,
        schedule: Box<dyn LeaderScheduleProvider>,
    ) -> Self {
        self.leaders_ahead = leaders_ahead;
        self.leader_schedule = Some(schedule);
        self
    }
}

#[async_trait]
impl Submitter for DualPathSubmitter {
    async fn submit(&self, bundle: Bundle) -> SubmitOutcome {
        let current_slot = bundle.sim.routed.candidate.slot;
        let should_send_jito = match &self.leader_schedule {
            Some(schedule) => {
                let has_jito = schedule.has_jito_leader_in_window(current_slot, self.leaders_ahead);
                if !has_jito {
                    tracing::warn!(
                        slot = current_slot,
                        leaders_ahead = self.leaders_ahead,
                        "Skipping Jito submission leg: no Jito-Solana validator in the next leaders_ahead window"
                    );
                }
                has_jito
            }
            None => true,
        };

        if !should_send_jito {
            // Jito leg skipped: only staked QUIC leg fires
            let staked_result = match tokio::time::timeout(self.timeout, self.staked_quic.send(&bundle)).await {
                Ok(res) => res,
                Err(_) => PathOutcome::TimedOut,
            };
            return match staked_result {
                PathOutcome::Landed { slot, actual_net_usd } => SubmitOutcome::Landed { slot, actual_net_usd },
                PathOutcome::Reverted { reason } => SubmitOutcome::Reverted { reason },
                PathOutcome::TimedOut => SubmitOutcome::NotIncluded,
            };
        }

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

    #[tokio::test]
    async fn test_get_tip_accounts_fallback_and_live_refresh() {
        assert_eq!(DEFAULT_JITO_TIP_ACCOUNTS.len(), 8);

        // Initialized tip manager contains the 8 verified defaults
        let manager = JitoTipManager::new("http://127.0.0.1:19999");
        let initial = manager.get_active_tip_accounts();
        assert_eq!(initial.len(), 8);
        assert_eq!(initial[0], DEFAULT_JITO_TIP_ACCOUNTS[0]);

        // Direct update works
        let custom = vec!["CustomTip1".to_string(), "CustomTip2".to_string()];
        manager.update_accounts(custom.clone());
        assert_eq!(manager.get_active_tip_accounts(), custom);

        // Failed live call triggers fallback back to the 8 verified defaults
        let res = manager.refresh_tip_accounts().await;
        assert!(res.is_err());
        let fallback = manager.get_active_tip_accounts();
        assert_eq!(fallback.len(), 8);
        assert_eq!(fallback[0], DEFAULT_JITO_TIP_ACCOUNTS[0]);
    }

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct TrackedPath {
        called: Arc<AtomicBool>,
        outcome: PathOutcome,
        delay: Duration,
        name: &'static str,
    }

    #[async_trait]
    impl SendPath for TrackedPath {
        async fn send(&self, _bundle: &Bundle) -> PathOutcome {
            self.called.store(true, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.outcome.clone()
        }
        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[tokio::test]
    async fn test_leader_awareness_skips_jito_leg_when_no_jito_leader() {
        let staked_called = Arc::new(AtomicBool::new(false));
        let jito_called = Arc::new(AtomicBool::new(false));

        let staked = Box::new(TrackedPath {
            called: staked_called.clone(),
            outcome: PathOutcome::Landed { slot: 101, actual_net_usd: 50.0 },
            delay: Duration::from_millis(5),
            name: "staked_quic",
        });
        let jito = Box::new(TrackedPath {
            called: jito_called.clone(),
            outcome: PathOutcome::Landed { slot: 101, actual_net_usd: 50.0 },
            delay: Duration::from_millis(5),
            name: "jito",
        });

        // Bundle is for slot 100, schedule only has Jito on slot 200 (far ahead)
        let schedule = Box::new(StaticLeaderSchedule::new([200]));
        let submitter = DualPathSubmitter::new(staked, jito, Duration::from_secs(1))
            .with_leader_schedule(2, schedule);

        let outcome = submitter.submit(dummy_bundle()).await;
        assert!(matches!(outcome, SubmitOutcome::Landed { slot: 101, .. }));

        // Staked QUIC must have fired; Jito must NOT have fired
        assert!(staked_called.load(Ordering::SeqCst));
        assert!(!jito_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_leader_awareness_fires_jito_leg_when_jito_leader_present() {
        let staked_called = Arc::new(AtomicBool::new(false));
        let jito_called = Arc::new(AtomicBool::new(false));

        let staked = Box::new(TrackedPath {
            called: staked_called.clone(),
            outcome: PathOutcome::Landed { slot: 101, actual_net_usd: 50.0 },
            delay: Duration::from_millis(5),
            name: "staked_quic",
        });
        let jito = Box::new(TrackedPath {
            called: jito_called.clone(),
            outcome: PathOutcome::Landed { slot: 101, actual_net_usd: 50.0 },
            delay: Duration::from_millis(5),
            name: "jito",
        });

        // Bundle is for slot 100, schedule has Jito validator on slot 101 (within 2 slots)
        let schedule = Box::new(StaticLeaderSchedule::new([101]));
        let submitter = DualPathSubmitter::new(staked, jito, Duration::from_secs(1))
            .with_leader_schedule(2, schedule);

        let outcome = submitter.submit(dummy_bundle()).await;
        assert!(matches!(outcome, SubmitOutcome::Landed { slot: 101, .. }));

        // Both legs must have fired
        assert!(staked_called.load(Ordering::SeqCst));
        assert!(jito_called.load(Ordering::SeqCst));
    }
}
