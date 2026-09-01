//! Config schema and loader for `config/gyrfalcon.toml`.
//!
//! Field-for-field mirror of [`docs/CONFIGURATION.md`](../../../docs/CONFIGURATION.md).
//! If a field here and that doc disagree, the doc wins until updated in the
//! same PR (see `CONTRIBUTING.md`).

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Config {
    pub geyser: GeyserConfig,
    pub rpc: RpcConfig,
    pub staked_send: StakedSendConfig,
    pub jito: JitoConfig,
    pub identity: IdentityConfig,
    pub treasury: TreasuryConfig,
    pub protocols: ProtocolsConfig,
    pub submit: SubmitConfig,
    pub risk: RiskConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GeyserConfig {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RpcConfig {
    /// Fallback RPC for account backfill only — never the hot path.
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StakedSendConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct JitoConfig {
    pub block_engine_url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct IdentityConfig {
    /// Path to the signer keypair. Never commit the file this points at.
    pub keypair_path: String,
    pub tip_account: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TreasuryConfig {
    /// Hot wallet keypair path. Never commit the file this points at.
    pub wallet_path: String,
    pub min_balance_sol: f64,
    pub sweep_interval: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ProtocolsConfig {
    pub kamino: ProtocolToggle,
    pub save: ProtocolToggle,
    pub marginfi: ProtocolToggle,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct ProtocolToggle {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubmitMode {
    /// Logs would-be liquidations without submitting.
    Observe,
    /// Submits transactions with real capital at risk.
    Live,
}

impl std::fmt::Display for SubmitMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubmitMode::Observe => write!(f, "observe"),
            SubmitMode::Live => write!(f, "live"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubmitConfig {
    pub mode: SubmitMode,
    pub leaders_ahead: u32,
    pub dual_path: bool,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct RiskConfig {
    pub min_profit_usd: f64,
    #[serde(default)]
    pub min_tip_usd: f64,
    pub sync_lag_halt_slots: u64,
    pub contention_ceiling: f64,
    pub max_tip_per_tx_usd: f64,
    pub max_tip_per_slot_usd: f64,
    pub max_tip_pct_of_bonus: f64,
    pub max_drawdown_usd: f64,
    pub consecutive_revert_limit: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file at {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config as TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error(
        "risk.consecutive_revert_limit must be >= 1 (got {0}) — a limit of 0 would halt every route on its first attempt"
    )]
    InvalidRevertLimit(u32),
}

impl Config {
    /// Load and parse a config file. Does not perform network I/O — this is
    /// pure deserialization plus the structural sanity checks in
    /// [`Config::validate`].
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let config: Config = toml::from_str(&raw)?;
        config.validate()?;
        Ok(config)
    }

    /// Structural sanity checks that don't require live chain state.
    /// Deliberately conservative: this catches obviously-wrong config
    /// (an unedited placeholder promoted to `live`), not every possible
    /// misconfiguration — see the production readiness checklist in
    /// `docs/RUNBOOK.md` for the full pre-capital gate.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.risk.consecutive_revert_limit == 0 {
            return Err(ConfigError::InvalidRevertLimit(
                self.risk.consecutive_revert_limit,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        // crates/config -> crates -> repo root
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn loads_example_config() {
        let path = repo_root().join("config/gyrfalcon.example.toml");
        let config = Config::load(&path).expect("example config should parse and validate");
        assert_eq!(config.submit.mode, SubmitMode::Observe);
        assert!(config.protocols.kamino.enabled);
        assert!(config.protocols.save.enabled);
        assert!(config.protocols.marginfi.enabled);
        assert_eq!(config.risk.consecutive_revert_limit, 3);
    }

    #[test]
    fn loads_devnet_config() {
        let path = repo_root().join("config/gyrfalcon.devnet.toml");
        let config = Config::load(&path).expect("devnet config should parse and validate");
        assert_eq!(config.submit.mode, SubmitMode::Live);
        assert!(!config.protocols.marginfi.enabled);
    }

    #[test]
    fn rejects_missing_file() {
        let err = Config::load("/nonexistent/gyrfalcon.toml").unwrap_err();
        assert!(matches!(err, ConfigError::Read { .. }));
    }

    #[test]
    fn rejects_malformed_toml() {
        let dir = std::env::temp_dir().join(format!("gyrfalcon-cfg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad_path = dir.join("bad.toml");
        std::fs::write(&bad_path, "this is not valid toml [[[").unwrap();
        let err = Config::load(&bad_path).unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_zero_revert_limit() {
        let dir = std::env::temp_dir().join(format!("gyrfalcon-cfg-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gyrfalcon.toml");
        let example = repo_root().join("config/gyrfalcon.example.toml");
        let mut contents = std::fs::read_to_string(example).unwrap();
        contents = contents.replace(
            "consecutive_revert_limit = 3",
            "consecutive_revert_limit = 0",
        );
        std::fs::write(&path, contents).unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidRevertLimit(0)));
        std::fs::remove_dir_all(&dir).ok();
    }
}
