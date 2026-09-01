//! Embedded, single-process state store — not a networked database, which
//! would sit on the hot path. See `docs/API.md#state-store`.

pub mod liquidation_log;
pub mod position_book;

pub use liquidation_log::{AsyncLiquidationWriter, ExportFormat, LiquidationLog};
pub use position_book::{PositionBook, PositionRecord, StoreError};
