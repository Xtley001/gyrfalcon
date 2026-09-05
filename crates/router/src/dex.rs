//! Multi-venue DEX routing engine per `03_ROUTING_DEX.md`.
//!
//! Evaluates Raydium CLMM, Orca Whirlpool, Sanctum (JitoSOL/SOL),
//! and Marinade (mSOL/SOL).

use gyrfalcon_core::Pubkey as CorePubkey;
use serde::{Deserialize, Serialize};

/// Supported DEX venues on Solana Mainnet-Beta per `03_ROUTING_DEX.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexVenue {
    RaydiumClmm,
    OrcaWhirlpool,
    Sanctum,
    Marinade,
}

impl DexVenue {
    pub fn is_direct(&self) -> bool {
        true
    }

    pub fn program_id(&self) -> &'static str {
        match self {
            DexVenue::RaydiumClmm => "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
            DexVenue::OrcaWhirlpool => "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
            DexVenue::Sanctum => "5ocnV1qiCgaQR8Jb8xWnVbApNzpWCDveWUig21uT3J9z",
            DexVenue::Marinade => "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD",
        }
    }
}

/// A quote from a DEX venue for a candidate swap.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketQuote {
    pub venue: DexVenue,
    pub market_address: CorePubkey,
    pub available_depth: u64,
    /// Expected tokens out after fees and slippage.
    pub expected_out_amount: u64,
    pub fee_bps: u32,
    pub price_impact_bps: u32,
}

/// The decision produced by `DexRouter::select_route`.
#[derive(Debug, Clone, PartialEq)]
pub struct DexRouteDecision {
    pub selected_venue: DexVenue,
    pub market_address: CorePubkey,
    pub expected_out_amount: u64,
    pub effective_price: f64,
    /// Audit trail: all candidate venues evaluated and their quotes per 03_ROUTING_DEX.md §4.
    pub considered_venues: Vec<MarketQuote>,
    pub fallback_used: bool,
}

/// DEX router selecting between direct AMMs and redemption routes.
pub struct DexRouter {
    max_price_impact_bps: u32,
    markets: std::collections::HashMap<(CorePubkey, CorePubkey), Vec<MarketQuote>>,
}

impl DexRouter {
    pub fn new(max_price_impact_bps: u32) -> Self {
        Self {
            max_price_impact_bps,
            markets: std::collections::HashMap::new(),
        }
    }

    pub fn register_market_quote(
        &mut self,
        input_mint: CorePubkey,
        output_mint: CorePubkey,
        quote: MarketQuote,
    ) {
        self.markets
            .entry((input_mint, output_mint))
            .or_default()
            .push(quote);
    }

    /// Select optimal DEX swap route per `03_ROUTING_DEX.md §4`:
    /// 1. Compares all direct venues with sufficient depth (`available_depth >= amount_in`).
    /// 2. Filters out venues exceeding `max_price_impact_bps`.
    /// 3. Picks venue with highest net `expected_out_amount` (best realized price, not simply deepest).
    /// 4. Ties break deterministically by venue registration order (first registered wins).
    /// 5. Returns `None` if no direct venue has sufficient depth within `max_price_impact_bps`.
    /// 6. Returns decision with complete audit trail of considered venues.
    pub fn select_route(
        &self,
        input_mint: CorePubkey,
        output_mint: CorePubkey,
        amount_in: u64,
    ) -> Option<DexRouteDecision> {
        let quotes = self.markets.get(&(input_mint, output_mint))?;
        if quotes.is_empty() {
            return None;
        }

        let mut candidates: Vec<(usize, &MarketQuote)> = quotes
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                q.venue.is_direct()
                    && q.available_depth >= amount_in
                    && q.price_impact_bps <= self.max_price_impact_bps
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Sort candidates by expected_out_amount descending; ties broken by registration order
        candidates.sort_by(|(idx_a, a), (idx_b, b)| {
            b.expected_out_amount
                .cmp(&a.expected_out_amount)
                .then(idx_a.cmp(idx_b))
        });

        let (_, winner) = candidates.first()?;

        let effective_price = if amount_in > 0 {
            winner.expected_out_amount as f64 / amount_in as f64
        } else {
            0.0
        };

        let decision = DexRouteDecision {
            selected_venue: winner.venue,
            market_address: winner.market_address,
            expected_out_amount: winner.expected_out_amount,
            effective_price,
            considered_venues: quotes.clone(),
            fallback_used: false,
        };

        Some(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(val: u8) -> CorePubkey {
        CorePubkey::new([val; 32])
    }

    #[test]
    fn test_select_route_picks_best_net_price_not_deepest() {
        let mut router = DexRouter::new(200); // 2% max impact
        let in_mint = pk(1);
        let out_mint = pk(2);

        // Venue 1: Orca Whirlpool — massive depth (100M), but higher fee/worse price (950k out)
        router.register_market_quote(
            in_mint,
            out_mint,
            MarketQuote {
                venue: DexVenue::OrcaWhirlpool,
                market_address: pk(10),
                available_depth: 100_000_000,
                expected_out_amount: 950_000,
                fee_bps: 30,
                price_impact_bps: 50,
            },
        );

        // Venue 2: Raydium CLMM — moderate depth (2M, sufficient for 1M trade), better price (990k out)
        router.register_market_quote(
            in_mint,
            out_mint,
            MarketQuote {
                venue: DexVenue::RaydiumClmm,
                market_address: pk(20),
                available_depth: 2_000_000,
                expected_out_amount: 990_000,
                fee_bps: 5,
                price_impact_bps: 10,
            },
        );

        let decision = router.select_route(in_mint, out_mint, 1_000_000).expect("must route");
        assert_eq!(decision.selected_venue, DexVenue::RaydiumClmm);
        assert_eq!(decision.expected_out_amount, 990_000);
        assert!(!decision.fallback_used);
        assert_eq!(decision.considered_venues.len(), 2);
    }

    #[test]
    fn test_select_route_breaks_ties_by_registration_order() {
        let mut router = DexRouter::new(200);
        let in_mint = pk(3);
        let out_mint = pk(4);

        // First registered venue: Raydium CLMM offers 980k out
        router.register_market_quote(
            in_mint,
            out_mint,
            MarketQuote {
                venue: DexVenue::RaydiumClmm,
                market_address: pk(30),
                available_depth: 5_000_000,
                expected_out_amount: 980_000,
                fee_bps: 20,
                price_impact_bps: 20,
            },
        );

        // Second registered venue: Orca Whirlpool also offers 980k out (tied net price)
        router.register_market_quote(
            in_mint,
            out_mint,
            MarketQuote {
                venue: DexVenue::OrcaWhirlpool,
                market_address: pk(40),
                available_depth: 5_000_000,
                expected_out_amount: 980_000,
                fee_bps: 20,
                price_impact_bps: 20,
            },
        );

        // Deterministic tie-break per 03_ROUTING_DEX.md §4: first registered wins
        let decision = router.select_route(in_mint, out_mint, 1_000_000).expect("must route");
        assert_eq!(decision.selected_venue, DexVenue::RaydiumClmm);
        assert_eq!(decision.market_address, pk(30));
    }

    #[test]
    fn test_select_route_returns_none_when_venues_lack_depth() {
        let mut router = DexRouter::new(200);
        let in_mint = pk(5);
        let out_mint = pk(6);

        // Venue only has 3M depth
        router.register_market_quote(
            in_mint,
            out_mint,
            MarketQuote {
                venue: DexVenue::Sanctum,
                market_address: pk(50),
                available_depth: 3_000_000,
                expected_out_amount: 2_900_000,
                fee_bps: 25,
                price_impact_bps: 30,
            },
        );

        // Requesting 10M trade -> venue rejected for insufficient depth -> returns None (no fallback aggregator)
        assert!(router.select_route(in_mint, out_mint, 10_000_000).is_none());
    }

    #[test]
    fn test_select_route_audit_trail_records_rejected_venues() {
        let mut router = DexRouter::new(200);
        let in_mint = pk(7);
        let out_mint = pk(8);

        router.register_market_quote(
            in_mint,
            out_mint,
            MarketQuote {
                venue: DexVenue::Sanctum,
                market_address: pk(70),
                available_depth: 10_000_000,
                expected_out_amount: 995_000,
                fee_bps: 5,
                price_impact_bps: 2,
            },
        );
        router.register_market_quote(
            in_mint,
            out_mint,
            MarketQuote {
                venue: DexVenue::OrcaWhirlpool,
                market_address: pk(71),
                available_depth: 10_000_000,
                expected_out_amount: 980_000,
                fee_bps: 20,
                price_impact_bps: 10,
            },
        );

        let decision = router.select_route(in_mint, out_mint, 1_000_000).expect("must route");
        assert_eq!(decision.considered_venues.len(), 2);
        assert!(decision.considered_venues.iter().any(|q| q.venue == DexVenue::Sanctum));
        assert!(decision.considered_venues.iter().any(|q| q.venue == DexVenue::OrcaWhirlpool));
    }
}
