#![cfg(test)]

mod anchor_health_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Bytes, Env, String,
    };

    use crate::contract::{AnchorKitContract, AnchorKitContractClient};

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

    /// Set up a contract with an admin and a registered anchor (via set_anchor_metadata).
    fn setup(env: &Env) -> (Address, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);

        let admin = Address::generate(env);
        let anchor = Address::generate(env);

        client.initialize(&admin);

        // Register the anchor so it appears in ANCHLIST.
        client.set_anchor_metadata(&anchor, &80u32, &500u64, &70u32, &95u32, &1_000_000u64);

        (contract_id, anchor)
    }

    // -----------------------------------------------------------------------
    // record_endpoint_success
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_success_increments_count() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.record_endpoint_success(&anchor);
        let metrics = client.get_anchor_health(&anchor);

        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.failure_count, 0);
        assert_eq!(metrics.consecutive_failures, 0);
        assert_eq!(metrics.last_response_at, 1_000_000);
        assert_eq!(metrics.last_success_at, 1_000_000);
        assert_eq!(metrics.uptime_bps, 10_000); // 100 %
    }

    #[test]
    fn test_multiple_successes_accumulate() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.record_endpoint_success(&anchor);
        client.record_endpoint_success(&anchor);
        client.record_endpoint_success(&anchor);

        let metrics = client.get_anchor_health(&anchor);
        assert_eq!(metrics.success_count, 3);
        assert_eq!(metrics.uptime_bps, 10_000);
    }

    // -----------------------------------------------------------------------
    // record_endpoint_failure
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_failure_increments_count() {
        let env = make_env();
        set_ledger(&env, 2_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.record_endpoint_failure(&anchor);
        let metrics = client.get_anchor_health(&anchor);

        assert_eq!(metrics.failure_count, 1);
        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.consecutive_failures, 1);
        assert_eq!(metrics.last_response_at, 2_000_000);
        assert_eq!(metrics.last_success_at, 0);
        assert_eq!(metrics.uptime_bps, 0); // 0 %
    }

    #[test]
    fn test_consecutive_failures_reset_on_success() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.record_endpoint_failure(&anchor);
        client.record_endpoint_failure(&anchor);
        client.record_endpoint_failure(&anchor);

        let before = client.get_anchor_health(&anchor);
        assert_eq!(before.consecutive_failures, 3);

        client.record_endpoint_success(&anchor);
        let after = client.get_anchor_health(&anchor);
        assert_eq!(after.consecutive_failures, 0);
    }

    // -----------------------------------------------------------------------
    // uptime_bps calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_uptime_bps_mixed_results() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        // 3 successes, 1 failure → 75 % → 7500 bps
        client.record_endpoint_success(&anchor);
        client.record_endpoint_success(&anchor);
        client.record_endpoint_success(&anchor);
        client.record_endpoint_failure(&anchor);

        let metrics = client.get_anchor_health(&anchor);
        assert_eq!(metrics.success_count, 3);
        assert_eq!(metrics.failure_count, 1);
        assert_eq!(metrics.uptime_bps, 7500);
    }

    #[test]
    fn test_uptime_bps_all_failures() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.record_endpoint_failure(&anchor);
        client.record_endpoint_failure(&anchor);

        let metrics = client.get_anchor_health(&anchor);
        assert_eq!(metrics.uptime_bps, 0);
    }

    // -----------------------------------------------------------------------
    // get_anchor_health — not found
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic]
    fn test_get_anchor_health_no_data_panics() {
        let env = make_env();
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        // No success/failure recorded yet.
        client.get_anchor_health(&anchor);
    }

    // -----------------------------------------------------------------------
    // reset_anchor_health
    // -----------------------------------------------------------------------

    #[test]
    fn test_reset_anchor_health_clears_metrics() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.record_endpoint_success(&anchor);
        client.record_endpoint_success(&anchor);
        client.record_endpoint_failure(&anchor);

        client.reset_anchor_health(&anchor);
        let metrics = client.get_anchor_health(&anchor);

        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.failure_count, 0);
        assert_eq!(metrics.consecutive_failures, 0);
        assert_eq!(metrics.uptime_bps, 0);
        assert_eq!(metrics.last_response_at, 0);
        assert_eq!(metrics.last_success_at, 0);
    }

    // -----------------------------------------------------------------------
    // list_anchor_health
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_anchor_health_returns_only_anchors_with_data() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor_a = Address::generate(&env);
        let anchor_b = Address::generate(&env);

        client.initialize(&admin);
        client.set_anchor_metadata(&anchor_a, &80u32, &500u64, &70u32, &95u32, &1_000_000u64);
        client.set_anchor_metadata(&anchor_b, &60u32, &800u64, &50u32, &90u32, &500_000u64);

        // Only record health for anchor_a.
        client.record_endpoint_success(&anchor_a);

        let list = client.list_anchor_health();
        assert_eq!(list.len(), 1);
        assert_eq!(list.get(0).unwrap().anchor, anchor_a);
    }

    #[test]
    fn test_list_anchor_health_multiple_anchors() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let anchor_a = Address::generate(&env);
        let anchor_b = Address::generate(&env);

        client.initialize(&admin);
        client.set_anchor_metadata(&anchor_a, &80u32, &500u64, &70u32, &95u32, &1_000_000u64);
        client.set_anchor_metadata(&anchor_b, &60u32, &800u64, &50u32, &90u32, &500_000u64);

        client.record_endpoint_success(&anchor_a);
        client.record_endpoint_failure(&anchor_b);

        let list = client.list_anchor_health();
        assert_eq!(list.len(), 2);
    }

    // -----------------------------------------------------------------------
    // last_response_at tracks most recent call
    // -----------------------------------------------------------------------

    #[test]
    fn test_last_response_at_updates_on_each_call() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, anchor) = setup(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.record_endpoint_success(&anchor);
        assert_eq!(client.get_anchor_health(&anchor).last_response_at, 1_000_000);

        set_ledger(&env, 2_000_000);
        client.record_endpoint_failure(&anchor);
        assert_eq!(client.get_anchor_health(&anchor).last_response_at, 2_000_000);
    }
}
