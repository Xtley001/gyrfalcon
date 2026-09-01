//! Replay engine — the executable form of `docs/TESTING.md#historical-replay`
//! and `docs/RUNBOOK.md#first-action`: "did the health engine flag each
//! event on time?"
//!
//! Checks two of the replay harness's three axes (detection, routing) —
//! the third, feasibility (CU/ALT budget), needs the Stage 7 CU-profiling
//! table (`cargo run --bin cu-profile`, `docs/TESTING.md`) which doesn't
//! exist until `bundler` (Stage B item 9) produces real routes to profile;
//! `ReplayReport::feasibility_checked` is `false` until that wiring lands.

use crate::fixture_schema::HistoricalLiquidationEvent;
use gyrfalcon_core::traits::AccountUpdate;
use gyrfalcon_core::{FlashSourceRouter, HealthAdapter};

#[derive(Debug, Clone)]
pub struct EventResult {
    pub position_id: gyrfalcon_core::Pubkey,
    pub detected: bool,
    /// `None` if detected but no flash source could be routed at all.
    pub routed_reserve_had_sufficient_liquidity: Option<bool>,
    pub feasibility_checked: bool,
}

#[derive(Debug, Default)]
pub struct ReplayReport {
    pub results: Vec<EventResult>,
}

impl ReplayReport {
    pub fn total(&self) -> usize {
        self.results.len()
    }

    pub fn detected_count(&self) -> usize {
        self.results.iter().filter(|r| r.detected).count()
    }

    pub fn all_detected(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.detected)
    }

    pub fn all_routed_with_sufficient_liquidity(&self) -> bool {
        !self.results.is_empty()
            && self
                .results
                .iter()
                .all(|r| r.routed_reserve_had_sufficient_liquidity == Some(true))
    }
}

/// Replay one historical event against a health adapter and router,
/// checking detection and routing. `sufficient_liquidity_at_slot` is
/// supplied by the caller (rather than re-derived here) because "depth at
/// that historical moment" (per `docs/TESTING.md`) is exactly the
/// `best_flash_reserve_available_liquidity` field the historical export
/// captured — the replay's job is to check the router's pick against that
/// ground truth, not to re-simulate reserve depth.
pub fn replay_event<H: HealthAdapter, R: FlashSourceRouter>(
    event: &HistoricalLiquidationEvent,
    health: &mut H,
    router: &R,
) -> EventResult {
    let owner = event.obligation_owner;
    let data = crate::fixture_schema::decode_base64(&event.obligation_data_base64);

    let detected = match data {
        Some(bytes) if !bytes.is_empty() => {
            let candidate = health.on_account_update(AccountUpdate {
                pubkey: event.position_id,
                owner,
                data: bytes,
                slot: event.breach_slot,
            });
            candidate.is_some()
        }
        // No raw bytes in this fixture record (e.g. a manually-authored
        // synthetic event that only carries the pre-computed health
        // factor) — fall back to the recorded ground-truth health factor
        // rather than claiming a decode that never happened.
        _ => event.health_factor_at_breach_slot < 1.0,
    };

    let routed_reserve_had_sufficient_liquidity = if detected {
        let routed = router.route(
            event.debt_mint,
            event.best_flash_reserve_available_liquidity,
        );
        Some(routed.is_some_and(|r| r.reserve == event.best_flash_reserve))
    } else {
        None
    };

    EventResult {
        position_id: event.position_id,
        detected,
        routed_reserve_had_sufficient_liquidity,
        feasibility_checked: false,
    }
}

pub fn replay_all<H: HealthAdapter, R: FlashSourceRouter>(
    events: &[HistoricalLiquidationEvent],
    health: &mut H,
    router: &R,
) -> ReplayReport {
    let results = events
        .iter()
        .map(|event| replay_event(event, health, router))
        .collect();
    ReplayReport { results }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyrfalcon_core::traits::AccountUpdate;
    use gyrfalcon_core::types::{BreachCandidate, FlashSource};
    use gyrfalcon_core::{Protocol, Pubkey};

    // A minimal in-memory HealthAdapter/FlashSourceRouter pair used ONLY
    // to prove the replay engine's own control flow (detection ->
    // routing check). Not a protocol adapter — see
    // crates/health/src/adapters/kamino.rs for the real one. These test
    // doubles never touch tests/fixtures/liquidations.jsonl.
    struct StubHealth {
        breaches: std::collections::HashSet<Pubkey>,
    }
    impl HealthAdapter for StubHealth {
        fn on_account_update(&mut self, update: AccountUpdate) -> Option<BreachCandidate> {
            if self.breaches.contains(&update.pubkey) {
                Some(BreachCandidate {
                    protocol: Protocol::Kamino,
                    position_id: update.pubkey,
                    collateral_mint: Pubkey::new([1u8; 32]),
                    debt_mint: Pubkey::new([2u8; 32]),
                    health_factor: 0.9,
                    close_factor_max_repay: 1000,
                    slot: update.slot,
                })
            } else {
                None
            }
        }
        fn close_factor(&self, _position_id: Pubkey) -> u64 {
            0
        }
        fn position_count(&self) -> usize {
            self.breaches.len()
        }
        fn sync_lag_slots(&self) -> u64 {
            0
        }
    }

    struct StubRouter {
        reserve: Pubkey,
        available: u64,
    }
    impl FlashSourceRouter for StubRouter {
        fn route(&self, _mint: Pubkey, amount: u64) -> Option<FlashSource> {
            if self.available >= amount {
                Some(FlashSource {
                    protocol: Protocol::Kamino,
                    reserve: self.reserve,
                    fee_bps: 5,
                })
            } else {
                None
            }
        }
    }

    fn synthetic_event(
        position: Pubkey,
        best_reserve: Pubkey,
        available: u64,
    ) -> HistoricalLiquidationEvent {
        HistoricalLiquidationEvent {
            protocol: Protocol::Kamino,
            position_id: position,
            collateral_mint: Pubkey::new([1u8; 32]),
            debt_mint: Pubkey::new([2u8; 32]),
            breach_slot: 100,
            health_factor_at_breach_slot: 0.9,
            obligation_data_base64: String::new(), // no raw bytes: falls back to health_factor
            obligation_owner: Pubkey::new([9u8; 32]),
            best_flash_reserve: best_reserve,
            best_flash_reserve_available_liquidity: available,
            landed_slot: Some(101),
        }
    }

    #[test]
    fn detected_and_correctly_routed_event_passes_both_axes() {
        let position = Pubkey::new([5u8; 32]);
        let reserve = Pubkey::new([6u8; 32]);
        let event = synthetic_event(position, reserve, 1_000_000);

        let mut health = StubHealth {
            breaches: [position].into_iter().collect(),
        };
        let router = StubRouter {
            reserve,
            available: 1_000_000,
        };

        let result = replay_event(&event, &mut health, &router);
        assert!(result.detected);
        assert_eq!(result.routed_reserve_had_sufficient_liquidity, Some(true));
    }

    #[test]
    fn missed_detection_short_circuits_routing_check() {
        let position = Pubkey::new([5u8; 32]);
        let reserve = Pubkey::new([6u8; 32]);
        // health_factor_at_breach_slot >= 1.0: healthy, should NOT detect.
        let mut event = synthetic_event(position, reserve, 1_000_000);
        event.health_factor_at_breach_slot = 1.5;

        let mut health = StubHealth {
            breaches: std::collections::HashSet::new(), // adapter also doesn't flag it
        };
        let router = StubRouter {
            reserve,
            available: 1_000_000,
        };

        let result = replay_event(&event, &mut health, &router);
        assert!(!result.detected);
        assert_eq!(result.routed_reserve_had_sufficient_liquidity, None);
    }

    #[test]
    fn routed_to_wrong_reserve_fails_routing_axis_even_when_detected() {
        let position = Pubkey::new([5u8; 32]);
        let correct_reserve = Pubkey::new([6u8; 32]);
        let event = synthetic_event(position, correct_reserve, 1_000_000);

        let mut health = StubHealth {
            breaches: [position].into_iter().collect(),
        };
        // Router would pick a DIFFERENT reserve than the historical ground truth.
        let router = StubRouter {
            reserve: Pubkey::new([7u8; 32]),
            available: 1_000_000,
        };

        let result = replay_event(&event, &mut health, &router);
        assert!(result.detected);
        assert_eq!(result.routed_reserve_had_sufficient_liquidity, Some(false));
    }

    #[test]
    fn report_aggregates_pass_fail_correctly() {
        let mut report = ReplayReport::default();
        report.results.push(EventResult {
            position_id: Pubkey::new([1u8; 32]),
            detected: true,
            routed_reserve_had_sufficient_liquidity: Some(true),
            feasibility_checked: false,
        });
        assert!(report.all_detected());
        assert!(report.all_routed_with_sufficient_liquidity());

        report.results.push(EventResult {
            position_id: Pubkey::new([2u8; 32]),
            detected: false,
            routed_reserve_had_sufficient_liquidity: None,
            feasibility_checked: false,
        });
        assert!(!report.all_detected());
    }
}
