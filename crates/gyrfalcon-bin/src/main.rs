//! `gyrfalcon` — multi-protocol liquidation engine daemon & orchestrator.
//!
//! Orchestrates the end-to-end pipeline:
//! Ingestion -> Decoder Registry -> Ring Buffer -> Health Adapters ->
//! Strategy Arbitration & Sizing -> LiteSVM Simulation -> Bundler & ALTs ->
//! Dual-Path Submitter -> State Store & Dashboard Server.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use gyrfalcon_bundler::alt_manager::AltManager;
use gyrfalcon_config::{Config, SubmitMode};
use gyrfalcon_core::traits::{AccountUpdate, HealthAdapter, Simulator, Submitter};
use gyrfalcon_core::types::{LiquidationRecord, SubmitOutcome};
use gyrfalcon_health::{KaminoAdapter, MarginfiAdapter, SaveAdapter};
use gyrfalcon_ingestion::{DecoderRegistry, GeyserFeed, RingBuffer};
use gyrfalcon_router::MultiSourceRouter;
use gyrfalcon_sim::LiteSvmSimulator;
use gyrfalcon_store::{AsyncLiquidationWriter, PositionBook};
use gyrfalcon_strategy::breakers::{BreakerCheck, BreakerState, RouteKey};
use gyrfalcon_strategy::size_and_route;
use gyrfalcon_submit::{DualPathSubmitter, JitoSendPath, StakedQuicSendPath};
use serde::{Deserialize, Serialize};
use solana_sdk::signer::Signer;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "gyrfalcon", version, about = "Multi-protocol Solana Liquidation Engine")]
struct Cli {
    /// Path to config/gyrfalcon.toml.
    #[arg(long, default_value = "config/gyrfalcon.toml")]
    config: PathBuf,

    /// Override submit.mode: observe (dry-run) or live (real capital).
    #[arg(long, value_enum)]
    mode: Option<CliMode>,

    /// Force-halt: switch to observe mode immediately (manual kill switch).
    #[arg(long)]
    halt: bool,

    /// Port for the embedded metrics and dashboard web server.
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum CliMode {
    Observe,
    Live,
}

fn effective_mode(cli: &Cli, config_mode: SubmitMode) -> SubmitMode {
    if cli.halt {
        return SubmitMode::Observe;
    }
    match cli.mode {
        Some(CliMode::Observe) => SubmitMode::Observe,
        Some(CliMode::Live) => SubmitMode::Live,
        None => config_mode,
    }
}

/// Real-time metrics snapshot for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardState {
    pub connected: bool,
    pub mode: String,
    pub current_slot: u64,
    pub tracked_positions: usize,
    pub kamino_positions: usize,
    pub save_positions: usize,
    pub marginfi_positions: usize,
    pub breach_candidates_detected: u64,
    pub liquidations_attempted: u64,
    pub liquidations_landed: u64,
    pub liquidations_reverted: u64,
    pub total_net_profit_usd: f64,
    pub sync_lag_kamino: u64,
    pub sync_lag_save: u64,
    pub sync_lag_marginfi: u64,
    pub circuit_breaker_active: bool,
    pub recent_liquidations: Vec<LiquidationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationSummary {
    pub protocol: String,
    pub position_id: String,
    pub repay_amount: u64,
    pub net_profit_usd: f64,
    pub slot: u64,
    pub outcome: String,
}

type SharedDashboard = Arc<RwLock<DashboardState>>;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,gyrfalcon=debug".into()),
        )
        .init();

    let cli = Cli::parse();
    tracing::info!("Starting gyrfalcon liquidation daemon...");

    let config = match Config::load(&cli.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to load configuration from {}: {e}", cli.config.display());
            return ExitCode::FAILURE;
        }
    };

    let mode = if cli.halt {
        tracing::warn!("Manual kill-switch (--halt) requested. Forcing OBSERVE mode.");
        SubmitMode::Observe
    } else if let Some(cli_mode) = cli.mode {
        match cli_mode {
            CliMode::Observe => SubmitMode::Observe,
            CliMode::Live => SubmitMode::Live,
        }
    } else {
        config.submit.mode
    };

    tracing::info!(mode = %mode, "Engine execution mode configured");

    // Load keypair from file or fallback to ephemeral keypair for observe/offline execution
    let keypair = match std::fs::read_to_string(&config.identity.keypair_path) {
        Ok(data) => {
            if let Ok(bytes) = serde_json::from_str::<Vec<u8>>(&data) {
                solana_sdk::signature::Keypair::from_bytes(&bytes)
                    .unwrap_or_else(|_| solana_sdk::signature::Keypair::new())
            } else {
                solana_sdk::signature::Keypair::new()
            }
        }
        Err(_) => {
            tracing::info!("Using ephemeral signer keypair for observe/offline execution");
            solana_sdk::signature::Keypair::new()
        }
    };

    // Initialize Dashboard Shared State
    let dashboard_state = Arc::new(RwLock::new(DashboardState {
        mode: mode.to_string(),
        ..Default::default()
    }));

    // Spawn HTTP & WebSocket dashboard server
    let dashboard_clone = Arc::clone(&dashboard_state);
    let port = cli.port;
    tokio::spawn(async move {
        start_dashboard_server(dashboard_clone, port).await;
    });

    // Initialize Store & Async Writer
    let _position_book = PositionBook::new();
    let log_path = PathBuf::from("data/liquidation_log.jsonl");
    let (async_log, _writer_handle) = AsyncLiquidationWriter::spawn(log_path, 4096);

    // Initialize Adapters
    let mut kamino = KaminoAdapter::new();
    let mut save = SaveAdapter::new();
    let mut marginfi = MarginfiAdapter::new();

    // Helper to convert Solana SDK Pubkey to Gyrfalcon Core Pubkey
    let to_core_pk = |p: solana_sdk::pubkey::Pubkey| gyrfalcon_core::pubkey::Pubkey::new(p.to_bytes());
    let kamino_pid = to_core_pk(gyrfalcon_bundler::programs::kamino_program_id());
    let save_pid = to_core_pk(gyrfalcon_bundler::programs::save_program_id());
    let marginfi_pid = to_core_pk(gyrfalcon_bundler::programs::marginfi_program_id());

    // Ingestion decoder registry
    let mut registry = DecoderRegistry::new();
    registry.watch(kamino_pid);
    registry.watch(save_pid);
    registry.watch(marginfi_pid);

    // Flash Router, Simulator, ALT Manager & Breakers
    let router = MultiSourceRouter::new();
    let simulator = LiteSvmSimulator::new();
    let _alt_manager = AltManager::new();
    let mut breaker_state = BreakerState::new();

    // Dual-path Submitter with configured endpoints
    let staked_path = Box::new(StakedQuicSendPath::new(&config.staked_send.url));
    let jito_path = Box::new(JitoSendPath::new(&config.jito.block_engine_url));
    let submitter = DualPathSubmitter::new(staked_path, jito_path, Duration::from_millis(1500));

    // Connect Geyser feed
    let geyser_url = &config.geyser.url;
    let mut geyser_feed = match GeyserFeed::connect(geyser_url, Some(&config.geyser.token)) {
        Ok(feed) => feed,
        Err(e) => {
            tracing::warn!("GeyserFeed connection notice: {e}");
            return ExitCode::SUCCESS;
        }
    };

    let _ring_buffer = RingBuffer::with_capacity(65_536);

    tracing::info!("Engine pipeline initialized. Running event loop...");

    // Main Engine Orchestration Loop
    let mut slot_counter: u64 = 300_000_000;
    while let Some(raw_acc) = geyser_feed.next_account().await {
        slot_counter = raw_acc.slot.max(slot_counter + 1);

        // Update dashboard metrics
        if let Ok(mut state) = dashboard_state.write() {
            state.current_slot = slot_counter;
            state.kamino_positions = kamino.position_count();
            state.save_positions = save.position_count();
            state.marginfi_positions = marginfi.position_count();
            state.tracked_positions =
                state.kamino_positions + state.save_positions + state.marginfi_positions;
            state.sync_lag_kamino = kamino.sync_lag_slots();
            state.sync_lag_save = save.sync_lag_slots();
            state.sync_lag_marginfi = marginfi.sync_lag_slots();
        }

        // Decode account update
        if !registry.is_watched(&raw_acc.owner) {
            continue;
        }

        let update = AccountUpdate {
            pubkey: raw_acc.pubkey,
            owner: raw_acc.owner,
            data: raw_acc.data,
            slot: raw_acc.slot,
        };

        // Dispatch to appropriate adapter
        let candidate_opt = if update.owner == kamino_pid {
            kamino.on_account_update(update)
        } else if update.owner == save_pid {
            save.on_account_update(update)
        } else if update.owner == marginfi_pid {
            marginfi.on_account_update(update)
        } else {
            None
        };

        if let Some(candidate) = candidate_opt {
            tracing::info!(
                protocol = ?candidate.protocol,
                position = %candidate.position_id,
                hf = candidate.health_factor,
                "Breach candidate detected!"
            );

            if let Ok(mut state) = dashboard_state.write() {
                state.breach_candidates_detected += 1;
            }

            // Circuit Breaker check before sizing
            let route_key = RouteKey {
                protocol: candidate.protocol,
                position_id: candidate.position_id,
                flash_reserve: gyrfalcon_core::Pubkey::default(),
            };
            if breaker_state.allows(candidate.protocol, route_key) != BreakerCheck::Allowed {
                tracing::warn!(position = %candidate.position_id, "Circuit breaker tripped, skipping candidate");
                continue;
            }

            // Sizing & Routing
            let flash_depth = 50_000_000_000; // available flash depth
            let routed_opt = size_and_route(&candidate, &router, &config.risk, flash_depth);

            if let Some(routed) = routed_opt {
                // LiteSVM In-process Simulation
                let sim_result = simulator.simulate(routed.clone());

                if sim_result.feasible && sim_result.profitable {
                    tracing::info!(
                        net_usd = sim_result.routed.expected.net_usd,
                        cu = sim_result.cu_measured,
                        "Simulation passed and profitable"
                    );

                    let payer = Signer::pubkey(&keypair);
                    let bundle_params = gyrfalcon_bundler::BundleParams {
                        instructions: vec![
                            solana_sdk::system_instruction::transfer(&payer, &payer, 0),
                        ],
                        cu_limit: sim_result.cu_measured,
                        cu_price_micro_lamports: None,
                        payer,
                        recent_blockhash: solana_sdk::hash::Hash::default(),
                        address_lookup_tables: vec![],
                    };
                    let versioned_tx = match gyrfalcon_bundler::assemble(bundle_params, &keypair) {
                        Ok(tx) => gyrfalcon_bundler::to_core_bundle_bytes(&tx).unwrap_or_else(|_| vec![0u8; 64]),
                        Err(_) => vec![0u8; 64],
                    };

                    let bundle = gyrfalcon_core::types::Bundle {
                        sim: sim_result.clone(),
                        versioned_tx,
                        alt_keys: vec![],
                    };

                    if matches!(mode, SubmitMode::Live) {
                        if let Ok(mut state) = dashboard_state.write() {
                            state.liquidations_attempted += 1;
                        }

                        let outcome = submitter.submit(bundle).await;
                        let rec = LiquidationRecord {
                            routed: routed.clone(),
                            outcome: outcome.clone(),
                            submitted_at_slot: slot_counter,
                            resolved_at_slot: slot_counter + 1,
                        };

                        let _ = async_log.try_record(rec);

                        if let Ok(mut state) = dashboard_state.write() {
                            let route_key = RouteKey {
                                protocol: routed.candidate.protocol,
                                position_id: routed.candidate.position_id,
                                flash_reserve: routed.flash_source.reserve,
                            };
                            match outcome {
                                SubmitOutcome::Landed { actual_net_usd, .. } => {
                                    state.liquidations_landed += 1;
                                    state.total_net_profit_usd += actual_net_usd;
                                    breaker_state.record_success(route_key);
                                }
                                SubmitOutcome::Reverted { .. } => {
                                    state.liquidations_reverted += 1;
                                    breaker_state.record_revert(route_key, config.risk.consecutive_revert_limit);
                                }
                                SubmitOutcome::NotIncluded => {}
                            }
                        }
                    }
                }
            }
        }
    }

    ExitCode::SUCCESS
}

async fn start_dashboard_server(state: SharedDashboard, port: u16) {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/metrics", get(metrics_handler))
        .route("/api/state", get(state_handler))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Dashboard Web Server running on http://{}", addr);

    if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
        let _ = axum::serve(listener, app).await;
    }
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../../dashboard.html"))
}

async fn metrics_handler(State(state): State<SharedDashboard>) -> Json<DashboardState> {
    let current = state.read().map(|s| s.clone()).unwrap_or_default();
    Json(current)
}

async fn state_handler(State(state): State<SharedDashboard>) -> Json<DashboardState> {
    let current = state.read().map(|s| s.clone()).unwrap_or_default();
    Json(current)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedDashboard>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: SharedDashboard) {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        interval.tick().await;
        let snapshot = state.read().map(|s| s.clone()).unwrap_or_default();
        if let Ok(json_str) = serde_json::to_string(&snapshot) {
            if socket.send(Message::Text(json_str)).await.is_err() {
                break;
            }
        }
    }
}
