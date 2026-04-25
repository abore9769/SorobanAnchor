#![cfg(test)]

mod metadata_cache_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Env, String,
    };

    use crate::contract::{AnchorKitContract, AnchorKitContractClient, AnchorMetadata};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn set_ledger(env: &Env, timestamp: u64) {
        env.ledger().set(LedgerInfo {
            timestamp,
            protocol_version: 21,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6312000,
        });
    }

    fn sample_metadata(env: &Env, anchor: &Address) -> AnchorMetadata {
        AnchorMetadata {
            anchor: anchor.clone(),
            reputation_score: 9000,
            liquidity_score: 8500,
            uptime_percentage: 9900,
            total_volume: 1_000_000,
            average_settlement_time: 300,
            is_active: true,
        }
    }

    #[test]
    fn test_cache_not_found() {
        let env = make_env();
        set_ledger(&env, 0);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor = Address::generate(&env);
        client.initialize(&admin);

        let result = client.try_get_cached_metadata(&anchor);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_and_retrieve_metadata() {
        let env = make_env();
        set_ledger(&env, 0);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor = Address::generate(&env);
        client.initialize(&admin);

        let meta = sample_metadata(&env, &anchor);
        client.cache_metadata(&anchor, &meta, &3600u64);

        let retrieved = client.get_cached_metadata(&anchor);
        assert_eq!(retrieved.reputation_score, 9000);
        assert_eq!(retrieved.is_active, true);
    }

    #[test]
    fn test_cache_expiration() {
        let env = make_env();
        set_ledger(&env, 0);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor = Address::generate(&env);
        client.initialize(&admin);

        let meta = sample_metadata(&env, &anchor);
        client.cache_metadata(&anchor, &meta, &10u64);

        // advance past TTL
        set_ledger(&env, 11);
        let result = client.try_get_cached_metadata(&anchor);
        assert!(result.is_err());
    }

    #[test]
    fn test_manual_refresh() {
        let env = make_env();
        set_ledger(&env, 0);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor = Address::generate(&env);
        client.initialize(&admin);

        let meta = sample_metadata(&env, &anchor);
        client.cache_metadata(&anchor, &meta, &3600u64);

        // verify it's there
        let _ = client.get_cached_metadata(&anchor);

        // refresh (invalidate)
        client.refresh_metadata_cache(&anchor);

        // now it should be gone
        let result = client.try_get_cached_metadata(&anchor);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_capabilities() {
        let env = make_env();
        set_ledger(&env, 0);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor = Address::generate(&env);
        client.initialize(&admin);

        let toml_url = String::from_str(&env, "https://anchor.example/.well-known/stellar.toml");
        let caps = String::from_str(&env, "{\"deposits\":true,\"withdrawals\":true}");
        client.cache_capabilities(&anchor, &toml_url, &caps, &3600u64);

        let cached = client.get_cached_capabilities(&anchor);
        assert_eq!(cached.capabilities, caps);
        assert_eq!(cached.toml_url, toml_url);
    }

    #[test]
    fn test_capabilities_expiration() {
        let env = make_env();
        set_ledger(&env, 0);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor = Address::generate(&env);
        client.initialize(&admin);

        let toml_url = String::from_str(&env, "https://anchor.example/.well-known/stellar.toml");
        let caps = String::from_str(&env, "{\"deposits\":true}");
        client.cache_capabilities(&anchor, &toml_url, &caps, &5u64);

        set_ledger(&env, 6);
        let result = client.try_get_cached_capabilities(&anchor);
        assert!(result.is_err());
    }

    #[test]
    fn test_refresh_capabilities() {
        let env = make_env();
        set_ledger(&env, 0);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor = Address::generate(&env);
        client.initialize(&admin);

        let toml_url = String::from_str(&env, "https://anchor.example/.well-known/stellar.toml");
        let caps = String::from_str(&env, "{\"deposits\":true}");
        client.cache_capabilities(&anchor, &toml_url, &caps, &3600u64);

        client.refresh_capabilities_cache(&anchor);

        let result = client.try_get_cached_capabilities(&anchor);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Stale-while-revalidate tests (#170)
    // -----------------------------------------------------------------------

    fn setup_swr(env: &Env) -> (AnchorKitContractClient, Address, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let anchor = Address::generate(env);
        client.initialize(&admin);
        (client, admin, anchor)
    }

    #[test]
    fn test_swr_fresh_within_primary_ttl() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _, anchor) = setup_swr(&env);

        let meta = sample_metadata(&env, &anchor);
        // primary TTL = 100s, stale TTL = 50s
        client.cache_metadata_swr(&anchor, &meta, &100u64, &50u64);

        // At t=1050 (age=50): still within primary TTL
        set_ledger(&env, 1050);
        let (retrieved, needs_refresh) = client.get_cached_metadata_swr(&anchor);
        assert_eq!(retrieved.reputation_score, 9000);
        assert!(!needs_refresh);
    }

    #[test]
    fn test_swr_stale_within_grace_period() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _, anchor) = setup_swr(&env);

        let meta = sample_metadata(&env, &anchor);
        // primary TTL = 100s, stale TTL = 50s
        client.cache_metadata_swr(&anchor, &meta, &100u64, &50u64);

        // At t=1120 (age=120): past primary TTL (100), within stale window (150)
        set_ledger(&env, 1120);
        let (retrieved, needs_refresh) = client.get_cached_metadata_swr(&anchor);
        assert_eq!(retrieved.reputation_score, 9000);
        assert!(needs_refresh);
    }

    #[test]
    fn test_swr_expired_after_both_ttls() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _, anchor) = setup_swr(&env);

        let meta = sample_metadata(&env, &anchor);
        // primary TTL = 100s, stale TTL = 50s → total = 150s
        client.cache_metadata_swr(&anchor, &meta, &100u64, &50u64);

        // At t=1160 (age=160): past both TTLs → CacheExpired
        set_ledger(&env, 1160);
        let result = client.try_get_cached_metadata_swr(&anchor);
        assert!(result.is_err());
    }

    #[test]
    fn test_force_refresh_updates_entry_regardless_of_ttl() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _, anchor) = setup_swr(&env);

        let meta = sample_metadata(&env, &anchor);
        client.cache_metadata_swr(&anchor, &meta, &100u64, &50u64);

        // Advance into stale window
        set_ledger(&env, 1120);
        let (_, needs_refresh) = client.get_cached_metadata_swr(&anchor);
        assert!(needs_refresh);

        // Force refresh with updated metadata
        let mut updated = sample_metadata(&env, &anchor);
        updated.reputation_score = 9500;
        client.force_refresh_metadata(&anchor, &updated, &100u64, &50u64);

        // Should now be fresh with new data
        let (retrieved, needs_refresh_after) = client.get_cached_metadata_swr(&anchor);
        assert_eq!(retrieved.reputation_score, 9500);
        assert!(!needs_refresh_after);
    }

    #[test]
    fn test_force_refresh_resets_clocks_from_expired() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _, anchor) = setup_swr(&env);

        let meta = sample_metadata(&env, &anchor);
        client.cache_metadata_swr(&anchor, &meta, &10u64, &5u64);

        // Advance past full expiry
        set_ledger(&env, 1020);
        assert!(client.try_get_cached_metadata_swr(&anchor).is_err());

        // Force refresh re-populates the cache
        client.force_refresh_metadata(&anchor, &meta, &100u64, &50u64);
        let (_, needs_refresh) = client.get_cached_metadata_swr(&anchor);
        assert!(!needs_refresh);
    }
}

