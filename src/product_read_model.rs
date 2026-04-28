//! Contract-backed read model for product verification (issue #304).
//!
//! Provides a normalized view of on-chain product and attestation data with an
//! optional in-process cache layer. The cache can be bypassed by passing
//! `bypass_cache = true` for debugging or consistency-sensitive reads.
//!
//! # Consistency guarantees
//! - Cached entries are valid for up to `CACHE_TTL_SECONDS`.
//! - Stale entries are evicted on the next read for that product.
//! - On-chain data is the source of truth; the cache is a read-through layer.

extern crate alloc;
use alloc::{collections::BTreeMap, string::String, vec::Vec};

/// Seconds a cached entry remains valid before re-querying the contract.
pub const CACHE_TTL_SECONDS: u64 = 60;

/// Normalized product verification response consumed by backend routes.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductVerification {
    pub product_id: String,
    pub verified: bool,
    pub attestation_count: u32,
    /// Unix timestamp of the most recent on-chain attestation, if any.
    pub latest_attestation_ts: Option<u64>,
    /// Human-readable badge label derived from on-chain state.
    pub badge: String,
}

/// Stub representing a raw on-chain product record returned by the contract.
#[derive(Debug, Clone)]
pub struct OnChainProduct {
    pub product_id: String,
    pub attestation_count: u32,
    pub latest_attestation_ts: Option<u64>,
}

/// Trait abstracting the contract query so tests can inject stubs.
pub trait ContractProductQuery {
    /// Fetch raw product data from the contract (or a stub).
    /// Returns `None` when the product is not found on-chain.
    fn query_product(&self, product_id: &str) -> Option<OnChainProduct>;
}

struct CacheEntry {
    value: ProductVerification,
    expires_at: u64,
}

/// Read model service with optional TTL cache.
pub struct ProductReadModel<Q: ContractProductQuery> {
    query: Q,
    cache: BTreeMap<String, CacheEntry>,
}

impl<Q: ContractProductQuery> ProductReadModel<Q> {
    pub fn new(query: Q) -> Self {
        Self { query, cache: BTreeMap::new() }
    }

    /// Return a normalized `ProductVerification`.
    ///
    /// Uses the cache unless `bypass_cache` is `true` or the entry is stale.
    pub fn get_verification(
        &mut self,
        product_id: &str,
        now: u64,
        bypass_cache: bool,
    ) -> ProductVerification {
        if !bypass_cache {
            if let Some(entry) = self.cache.get(product_id) {
                if entry.expires_at > now {
                    return entry.value.clone();
                }
            }
        }

        let result = self.fetch_and_normalize(product_id);
        self.cache.insert(
            product_id.to_string(),
            CacheEntry { value: result.clone(), expires_at: now + CACHE_TTL_SECONDS },
        );
        result
    }

    /// Explicitly invalidate a cached entry.
    pub fn invalidate(&mut self, product_id: &str) {
        self.cache.remove(product_id);
    }

    fn fetch_and_normalize(&self, product_id: &str) -> ProductVerification {
        match self.query.query_product(product_id) {
            Some(p) => {
                let badge = if p.attestation_count >= 3 {
                    "verified-gold"
                } else if p.attestation_count >= 1 {
                    "verified"
                } else {
                    "unverified"
                };
                ProductVerification {
                    product_id: p.product_id,
                    verified: p.attestation_count > 0,
                    attestation_count: p.attestation_count,
                    latest_attestation_ts: p.latest_attestation_ts,
                    badge: badge.into(),
                }
            }
            // Partial / missing on-chain data: return safe fallback
            None => ProductVerification {
                product_id: product_id.into(),
                verified: false,
                attestation_count: 0,
                latest_attestation_ts: None,
                badge: "unverified".into(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: collect all product IDs currently in the cache (for observability)
// ---------------------------------------------------------------------------
impl<Q: ContractProductQuery> ProductReadModel<Q> {
    pub fn cached_ids(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }
}
