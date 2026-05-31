#![cfg(test)]

mod diagnostics_tests {
    use soroban_sdk::{
        testutils::{Address as _, LedgerInfo},
        Address, Env,
    };

    use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient, AnchorMetadata};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1_000,
            protocol_version: 21,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });
        env
    }

    fn setup(env: &Env) -> (AnchorKitContractClient, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    fn sample_metadata(env: &Env, anchor: &Address) -> AnchorMetadata {
        AnchorMetadata {
            anchor: anchor.clone(),
            reputation_score: 9000,
            liquidity_score: 8000,
            uptime_percentage: 9900,
            total_volume: 500_000,
            average_settlement_time: 60,
            is_active: true,
        }
    }

    // -----------------------------------------------------------------------
    // Contract diagnostics: uninitialized contract
    // -----------------------------------------------------------------------

    #[test]
    fn test_contract_diagnostics_uninitialized() {
        let env = make_env();
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let diag = client.get_contract_diagnostics();
        assert!(!diag.is_initialized);
        assert_eq!(diag.total_attestations, 0);
        assert_eq!(diag.total_quotes, 0);
        assert_eq!(diag.total_sessions, 0);
    }

    // -----------------------------------------------------------------------
    // Contract diagnostics: initialized contract
    // -----------------------------------------------------------------------

    #[test]
    fn test_contract_diagnostics_initialized() {
        let env = make_env();
        let (client, _) = setup(&env);

        let diag = client.get_contract_diagnostics();
        assert!(diag.is_initialized);
        assert_eq!(diag.total_attestations, 0);
        assert_eq!(diag.rate_limit_max_submissions, 10);
        assert_eq!(diag.rate_limit_window_length, 100);
    }

    // -----------------------------------------------------------------------
    // Rate-limiter diagnostics: fresh attestor has zero submissions
    // -----------------------------------------------------------------------

    #[test]
    fn test_rate_limiter_diagnostics_fresh_attestor() {
        let env = make_env();
        let (client, _) = setup(&env);
        let attestor = Address::generate(&env);

        let diag = client.get_rate_limiter_diagnostics(&attestor);
        assert_eq!(diag.submission_count, 0);
        assert!(!diag.is_at_limit);
        assert_eq!(diag.max_submissions, 10);
    }

    // -----------------------------------------------------------------------
    // Rate-limiter diagnostics: at-limit detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_rate_limiter_diagnostics_reflects_config() {
        let env = make_env();
        let (client, _) = setup(&env);
        let attestor = Address::generate(&env);

        let diag = client.get_rate_limiter_diagnostics(&attestor);
        assert_eq!(diag.max_submissions, 10);
        assert_eq!(diag.window_length, 100);
        assert!(!diag.is_at_limit);
    }

    // -----------------------------------------------------------------------
    // Cache diagnostics: no cached entries
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_diagnostics_empty() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        let diag = client.get_cache_diagnostics(&anchor);
        assert!(!diag.metadata_cached);
        assert_eq!(diag.metadata_age_seconds, 0);
        assert_eq!(diag.metadata_ttl_seconds, 0);
        assert!(!diag.capabilities_cached);
        assert_eq!(diag.capabilities_age_seconds, 0);
        assert_eq!(diag.capabilities_ttl_seconds, 0);
    }

    // -----------------------------------------------------------------------
    // Cache diagnostics: metadata present after caching
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_diagnostics_with_metadata() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let meta = sample_metadata(&env, &anchor);

        client.cache_metadata(&anchor, &meta, &3600u64);

        let diag = client.get_cache_diagnostics(&anchor);
        assert!(diag.metadata_cached);
        assert_eq!(diag.metadata_ttl_seconds, 3600);
        assert_eq!(diag.metadata_age_seconds, 0);
    }

    // -----------------------------------------------------------------------
    // Session diagnostics: count starts at zero
    // -----------------------------------------------------------------------

    #[test]
    fn test_session_diagnostics_zero_initially() {
        let env = make_env();
        let (client, _) = setup(&env);

        let diag = client.get_session_diagnostics();
        assert_eq!(diag.total_sessions_created, 0);
    }

    // -----------------------------------------------------------------------
    // Session diagnostics: count increments after session creation
    // -----------------------------------------------------------------------

    #[test]
    fn test_session_diagnostics_increments_on_create() {
        let env = make_env();
        let (client, _) = setup(&env);
        let user = Address::generate(&env);

        client.create_session(&user);

        let diag = client.get_session_diagnostics();
        assert_eq!(diag.total_sessions_created, 1);
    }
}
