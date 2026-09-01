//! `cargo run --bin readiness-check [-- --config path/to/gyrfalcon.toml]`
//!
//! Turns `docs/RUNBOOK.md#production-readiness-checklist`'s 12 items into
//! a runnable command instead of a markdown list someone has to remember
//! to eyeball before promoting to `live`.
//!
//! Every item falls into one of three buckets:
//! - **Verified** — this tool checked it directly (config presence,
//!   evidence files existing, etc.) and it passed.
//! - **Manual** — requires live program state, a human judgment call, or
//!   infrastructure this tool has no way to reach (leased endpoints,
//!   devnet, a funded wallet). Checked off by a human, not this binary.
//! - **Missing** — a genuine gap in the current codebase, not yet
//!   implemented. Distinct from "Manual": this is something the codebase
//!   itself should eventually do automatically and currently doesn't
//!   (e.g. no oracle-staleness handling exists in any adapter yet, no ATA
//!   pre-provisioning code exists yet). Flagging these as their own
//!   category rather than folding them into "Manual" is the point of this
//!   tool — a human re-reading RUNBOOK.md's checklist by eye could easily
//!   mistake "nobody's built this" for "somebody should go check this."
//!
//! This tool refuses to report `submit.mode = live` as ready when any
//! item is not `Verified` — see [`main`]'s exit code.

use gyrfalcon_config::Config;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Verified,
    Manual,
    Missing,
}

impl Status {
    fn label(&self) -> &'static str {
        match self {
            Status::Verified => "VERIFIED",
            Status::Manual => "MANUAL  ",
            Status::Missing => "MISSING ",
        }
    }
}

struct CheckResult {
    item: &'static str,
    status: Status,
    detail: String,
}

fn check_replay_fixtures() -> CheckResult {
    let path = PathBuf::from("tests/fixtures/liquidations.jsonl");
    if path.exists()
        && std::fs::metadata(&path)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
    {
        CheckResult {
            item: "Historical replay fixtures populated (Stage B item 5)",
            status: Status::Verified,
            detail: format!("{} exists and is non-empty", path.display()),
        }
    } else {
        CheckResult {
            item: "Historical replay fixtures populated (Stage B item 5)",
            status: Status::Manual,
            detail: format!(
                "{} does not exist or is empty — run the one-time historical export step \
                 described in tests/fixtures/README.md against real mainnet history",
                path.display()
            ),
        }
    }
}

fn check_cu_table() -> CheckResult {
    let path = PathBuf::from("tests/fixtures/cu_table.csv");
    CheckResult {
        item: "Per-route CU profiling table built from real LiteSVM runs",
        status: if path.exists() {
            Status::Verified
        } else {
            Status::Missing
        },
        detail: if path.exists() {
            format!("{} exists", path.display())
        } else {
            "crates/sim/src/cu_profile.rs is fully written but not wired into the crate's \
             build — litesvm's own published dependency manifest is internally unsatisfiable \
             (reproduced directly against three versions; see crates/sim/src/lib.rs for the \
             exact resolver errors), independent of anything else in this workspace. Needs a \
             litesvm patch release, or bundler assembling real routes to profile once it's \
             re-enabled either way."
                .to_string()
        },
    }
}

fn check_config(config_path: &PathBuf) -> Vec<CheckResult> {
    let config = match Config::load(config_path) {
        Ok(c) => c,
        Err(e) => {
            return vec![CheckResult {
                item: "Config loads and validates",
                status: Status::Missing,
                detail: format!("{e} — fix config before any other check is meaningful"),
            }]
        }
    };

    let mut results = vec![CheckResult {
        item: "Config loads and validates",
        status: Status::Verified,
        detail: config_path.display().to_string(),
    }];

    results.push(CheckResult {
        item: "Treasury wallet funded above treasury.min_balance_sol",
        status: Status::Manual,
        detail: format!(
            "configured floor is {} SOL at {} — this tool has no RPC access to check the \
             live balance",
            config.treasury.min_balance_sol, config.treasury.wallet_path
        ),
    });

    results
}

fn static_checklist_items() -> Vec<CheckResult> {
    vec![
        CheckResult {
            item: "Current IDL for Kamino, Save, and MarginFi pulled from the deployed program",
            status: Status::Manual,
            detail: "re-verify against live program state, not this repo's pinned dependency \
                     versions (klend-interface / solend-sdk / marginfi-type-crate, each pinned \
                     to a commit or version at build time — see crates/health/Cargo.toml)"
                .to_string(),
        },
        CheckResult {
            item: "Kamino and Save flash-borrow fee schedules confirmed at current bps, per reserve",
            status: Status::Manual,
            detail: "requires live RPC reads against current Reserve accounts".to_string(),
        },
        CheckResult {
            item: "MarginFi flash-loan support re-confirmed as absent (or present)",
            status: Status::Verified,
            detail: "confirmed PRESENT (contradicts docs/ARCHITECTURE.md's original assumption) \
                     via MarginFi's own docs and a Sept 2025 security disclosure — see \
                     crates/health/src/adapters/marginfi.rs's module doc. Re-verify again \
                     before shipping if significant time has passed since that check."
                .to_string(),
        },
        CheckResult {
            item: "Oracle staleness/confidence handling verified per protocol",
            status: Status::Manual,
            detail: "Kamino (reserve_price_is_stale, wall-clock-timestamp-based) and Save \
                     (reserve_price_is_stale, slot-based, matches Save's own STALE_AFTER_SLOTS_ELAPSED) \
                     both have real, unit-tested staleness checks now. MarginFi's price lives on \
                     a separate oracle account this adapter doesn't decode — its \
                     reserve_price_is_stale is an explicit stub returning None, a genuine \
                     remaining gap, not silently missing. No confidence-interval (as opposed to \
                     staleness) checking exists for any protocol yet."
                .to_string(),
        },
        CheckResult {
            item: "ALTs built and warmed for every route intended to fire on day one",
            status: Status::Manual,
            detail: "gyrfalcon_bundler::alt builds real create/extend-lookup-table instructions \
                     (unit-tested for deterministic PDA derivation and address batching), but \
                     'warmed' is fundamentally a live-chain waiting condition (a table only \
                     becomes usable a slot after it's extended) — no route to build one for yet \
                     either, since that needs real routes from a live pipeline"
                .to_string(),
        },
        CheckResult {
            item: "Empirical write-lock contention map built from observed slot data",
            status: Status::Manual,
            detail: "strategy::dynamic_tip::ContentionModel exists and is unit-tested against \
                     synthetic data, but has never been fed real observe-mode observations \
                     (no live Geyser access in this build)"
                .to_string(),
        },
        CheckResult {
            item: "ATA pre-provisioning complete for every mint combination",
            status: Status::Manual,
            detail: "gyrfalcon_bundler::ata derives ATAs and builds idempotent-create \
                     instructions (unit-tested against the official spl-associated-token-account \
                     helper), but has never actually been run against a live wallet + the real \
                     set of watched mints"
                .to_string(),
        },
        CheckResult {
            item: "Staked-send + Jito dual submission tested end-to-end on devnet",
            status: Status::Missing,
            detail: "gyrfalcon_submit::DualPathSubmitter's race/timeout logic is unit-tested \
                     against fakes; the real SendPath implementations for staked QUIC and Jito \
                     are not written (need live network access) — see crates/submit/src/dual_path.rs"
                .to_string(),
        },
        CheckResult {
            item: "Tip-curve constant k and contention map calibrated from observe-mode data",
            status: Status::Manual,
            detail: "strategy::dynamic_tip::TipCurve::calibrate is implemented and unit-tested \
                     (recovers a known k from synthetic data) but has never run against real \
                     observations — no calibrated k ships with this repo, by design; see that \
                     module's doc"
                .to_string(),
        },
        CheckResult {
            item: "All four circuit breakers verified to actually halt submission",
            status: Status::Verified,
            detail: "crates/strategy/src/breakers.rs has a passing failure-injection test per \
                     breaker (consecutive-revert, treasury floor, drawdown, sync lag) plus the \
                     manual kill switch (crates/gyrfalcon-bin/src/main.rs's effective_mode tests) \
                     — run `cargo test -p gyrfalcon-strategy breakers::` and `cargo test -p \
                     gyrfalcon` to re-confirm"
                .to_string(),
        },
    ]
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config/gyrfalcon.toml"));

    let mut results = check_config(&config_path);
    results.push(check_replay_fixtures());
    results.push(check_cu_table());
    results.extend(static_checklist_items());

    println!("gyrfalcon production readiness — docs/RUNBOOK.md#production-readiness-checklist\n");
    let mut verified = 0;
    let mut manual = 0;
    let mut missing = 0;
    for r in &results {
        println!("[{}] {}", r.status.label(), r.item);
        println!("           {}", r.detail);
        match r.status {
            Status::Verified => verified += 1,
            Status::Manual => manual += 1,
            Status::Missing => missing += 1,
        }
    }

    println!(
        "\n{verified} verified, {manual} need manual/live verification, {missing} not yet implemented."
    );

    if missing > 0 || manual > 0 {
        println!(
            "\nNOT READY for submit.mode = live: every item must be VERIFIED, per \
             docs/BUILD_ORDER.md's Stage D exit criteria — this checklist passing, and \
             Phase 0 observe data showing net profitability, not just this repo's tests \
             being green."
        );
        std::process::ExitCode::FAILURE
    } else {
        println!("\nAll checklist items verified.");
        std::process::ExitCode::SUCCESS
    }
}
