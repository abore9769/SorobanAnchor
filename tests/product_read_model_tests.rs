//! Integration tests for the contract-backed product verification read model.
//! Covers happy path, degraded dependency (missing on-chain data), cache TTL,
//! bypass, and invalidation. Closes #304.

#[cfg(test)]
mod product_read_model_tests {
    use anchorkit::product_read_model::{
        ContractProductQuery, OnChainProduct, ProductReadModel, CACHE_TTL_SECONDS,
    };

    // ── Stubs ────────────────────────────────────────────────────────────────

    struct FoundStub {
        count: u32,
        ts: Option<u64>,
    }
    impl ContractProductQuery for FoundStub {
        fn query_product(&self, product_id: &str) -> Option<OnChainProduct> {
            Some(OnChainProduct {
                product_id: product_id.into(),
                attestation_count: self.count,
                latest_attestation_ts: self.ts,
            })
        }
    }

    struct NotFoundStub;
    impl ContractProductQuery for NotFoundStub {
        fn query_product(&self, _: &str) -> Option<OnChainProduct> {
            None
        }
    }

    /// Stub that counts how many times the contract was queried.
    struct CountingStub {
        pub calls: core::cell::Cell<u32>,
    }
    impl ContractProductQuery for CountingStub {
        fn query_product(&self, product_id: &str) -> Option<OnChainProduct> {
            self.calls.set(self.calls.get() + 1);
            Some(OnChainProduct {
                product_id: product_id.into(),
                attestation_count: 1,
                latest_attestation_ts: Some(1_000_000),
            })
        }
    }

    // ── Happy path ───────────────────────────────────────────────────────────

    #[test]
    fn verified_product_returns_correct_badge() {
        let mut svc = ProductReadModel::new(FoundStub { count: 3, ts: Some(9999) });
        let v = svc.get_verification("prod-1", 1000, false);
        assert!(v.verified);
        assert_eq!(v.badge, "verified-gold");
        assert_eq!(v.attestation_count, 3);
        assert_eq!(v.latest_attestation_ts, Some(9999));
    }

    #[test]
    fn single_attestation_returns_verified_badge() {
        let mut svc = ProductReadModel::new(FoundStub { count: 1, ts: Some(1) });
        let v = svc.get_verification("prod-2", 1000, false);
        assert!(v.verified);
        assert_eq!(v.badge, "verified");
    }

    // ── Degraded dependency (partial / missing on-chain data) ────────────────

    #[test]
    fn missing_product_returns_unverified_fallback() {
        let mut svc = ProductReadModel::new(NotFoundStub);
        let v = svc.get_verification("ghost-prod", 1000, false);
        assert!(!v.verified);
        assert_eq!(v.badge, "unverified");
        assert_eq!(v.attestation_count, 0);
        assert_eq!(v.latest_attestation_ts, None);
    }

    // ── Cache behaviour ──────────────────────────────────────────────────────

    #[test]
    fn cache_hit_avoids_second_contract_call() {
        let stub = CountingStub { calls: core::cell::Cell::new(0) };
        let mut svc = ProductReadModel::new(stub);
        svc.get_verification("prod-3", 1000, false);
        svc.get_verification("prod-3", 1001, false); // still within TTL
        assert_eq!(svc.query.calls.get(), 1, "contract should be queried only once");
    }

    #[test]
    fn stale_cache_triggers_re_query() {
        let stub = CountingStub { calls: core::cell::Cell::new(0) };
        let mut svc = ProductReadModel::new(stub);
        svc.get_verification("prod-4", 1000, false);
        // Advance time past TTL
        svc.get_verification("prod-4", 1000 + CACHE_TTL_SECONDS + 1, false);
        assert_eq!(svc.query.calls.get(), 2, "stale entry should trigger re-query");
    }

    #[test]
    fn bypass_cache_always_queries_contract() {
        let stub = CountingStub { calls: core::cell::Cell::new(0) };
        let mut svc = ProductReadModel::new(stub);
        svc.get_verification("prod-5", 1000, false);
        svc.get_verification("prod-5", 1001, true); // bypass
        assert_eq!(svc.query.calls.get(), 2);
    }

    #[test]
    fn invalidate_removes_cached_entry() {
        let stub = CountingStub { calls: core::cell::Cell::new(0) };
        let mut svc = ProductReadModel::new(stub);
        svc.get_verification("prod-6", 1000, false);
        assert!(svc.cached_ids().contains(&"prod-6".to_string()));
        svc.invalidate("prod-6");
        assert!(!svc.cached_ids().contains(&"prod-6".to_string()));
        svc.get_verification("prod-6", 1001, false);
        assert_eq!(svc.query.calls.get(), 2);
    }
}
