# API Reference

Core traits, data structures, and interface specifications for the `gyrfalcon` workspace crates.

## Core Domain Traits (`gyrfalcon-core`)

### `HealthAdapter`

Interface implemented by protocol obligation decoders (`crates/health/src/adapters/kamino.rs`).

```rust
pub trait HealthAdapter: Send + Sync {
    /// Returns the lending protocol identifier (always Protocol::Kamino).
    fn protocol(&self) -> Protocol;

    /// Evaluates raw account updates from Yellowstone gRPC and emits a breach candidate if HF < 1.0.
    fn on_account_update(&self, pubkey: &Pubkey, data: &[u8], slot: u64) -> Option<BreachCandidate>;

    /// Decodes obligation health factor with 18 decimals WAD precision.
    fn compute_health_factor(&self, obligation: &ObligationState) -> Result<U256, HealthError>;

    /// Calculates close factor maximum allowed debt repayment amount in base units.
    fn close_factor_max_repay(&self, obligation: &ObligationState, debt_mint: &Pubkey) -> u64;
}
```

### `Simulator`

In-process deterministic transaction verification interface implemented by `LiteSvmSimulator` (`crates/sim`).

```rust
pub trait Simulator: Send + Sync {
    /// Simulates a compiled transaction against the slot-current in-memory bank.
    fn simulate(&self, tx: &VersionedTransaction) -> Result<SimResult, SimError>;
}
```

### `Submitter`

Dual-path submission interface implemented by `DualPathSubmitter` (`crates/submit`).

```rust
#[async_trait]
pub trait Submitter: Send + Sync {
    /// Concurrently submits a versioned transaction via Staked QUIC and Jito Block Engine.
    async fn submit(&self, tx: VersionedTransaction, metadata: SubmissionMetadata) -> Result<SubmitOutcome, SubmitError>;
}
```

## Strategy & Routing Interfaces (`gyrfalcon-strategy`, `gyrfalcon-router`)

### Position Sizing & Routing Engine

```rust
pub fn size_and_route(
    candidate: &BreachCandidate,
    flash_router: &FlashSourceRouter,
    dex_router: &DexRouter,
    risk_config: &RiskConfig,
    regime: GasRegime,
) -> Option<RoutedCandidate>;
```

### Route Selection Method

```rust
impl DexRouter {
    /// Selects the optimal DEX exit route sorted by net proceeds after slippage and pool fees.
    pub fn select_route(
        &self,
        collateral_mint: &Pubkey,
        debt_mint: &Pubkey,
        amount_in: u64,
        max_price_impact_bps: u16,
    ) -> Option<DexRouteDecision>;
}
```

## Core Domain Types

### Protocol & Venues

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    Kamino,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexVenue {
    RaydiumClmm,
    OrcaWhirlpool,
    Sanctum,
    Marinade,
}
```

### Candidates & Sizing

```rust
pub struct BreachCandidate {
    pub protocol: Protocol,
    pub market: Pubkey,
    pub obligation: Pubkey,
    pub borrower: Pubkey,
    pub health_factor: U256,
    pub collateral_reserve: Pubkey,
    pub collateral_mint: Pubkey,
    pub debt_reserve: Pubkey,
    pub debt_mint: Pubkey,
    pub close_factor_max_repay: u64,
    pub liquidation_bonus_bps: u16,
    pub slot: u64,
}

pub struct RoutedCandidate {
    pub breach: BreachCandidate,
    pub repay_amount: u64,
    pub flash_provider: FlashProvider,
    pub dex_decision: DexRouteDecision,
    pub estimated_gross_profit_usd: f64,
    pub tip_lamports: u64,
    pub binding_constraint: BindingConstraint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingConstraint {
    CloseFactor,
    FlashDepth,
    ByteLimit,
    ComputeBudget,
}
```

### Gas Regimes & Outcomes

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GasRegime {
    Normal,
    High,
    Spike,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimResult {
    pub feasible: bool,
    pub profitable: bool,
    pub cu_measured: u64,
    pub net_profit_usd: f64,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmitOutcome {
    Landed { signature: String, slot: u64 },
    Reverted { signature: String, slot: u64, error: String },
    NotIncluded { reason: String },
}
```
