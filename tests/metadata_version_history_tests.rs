#![cfg(test)]

mod metadata_version_history_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Env,
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

    fn setup(env: &Env) -> (AnchorKitContractClient, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    // -----------------------------------------------------------------------
    // Version counter tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_first_set_creates_version_1() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        client.set_anchor_metadata(&anchor, &8000u32, &300u64, &7500u32, &9900u32, &1_000_000u64);

        let count = client.get_anchor_meta_version_count(&anchor);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_multiple_updates_increment_version() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        client.set_anchor_metadata(&anchor, &8000u32, &300u64, &7500u32, &9900u32, &1_000_000u64);
        client.set_anchor_metadata(&anchor, &8500u32, &250u64, &8000u32, &9950u32, &2_000_000u64);
        client.set_anchor_metadata(&anchor, &9000u32, &200u64, &8500u32, &9980u32, &3_000_000u64);

        let count = client.get_anchor_meta_version_count(&anchor);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_version_count_zero_for_unknown_anchor() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        let count = client.get_anchor_meta_version_count(&anchor);
        assert_eq!(count, 0);
    }

    // -----------------------------------------------------------------------
    // History retrieval tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_history_returns_ordered_versions() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        client.set_anchor_metadata(&anchor, &1000u32, &600u64, &1000u32, &9000u32, &100_000u64);
        set_ledger(&env, 2_000);
        client.set_anchor_metadata(&anchor, &2000u32, &500u64, &2000u32, &9100u32, &200_000u64);
        set_ledger(&env, 3_000);
        client.set_anchor_metadata(&anchor, &3000u32, &400u64, &3000u32, &9200u32, &300_000u64);

        let history = client.get_anchor_metadata_history(&anchor, &10u32);
        assert_eq!(history.len(), 3);

        // Versions must be in ascending order
        assert_eq!(history.get(0).unwrap().version, 1);
        assert_eq!(history.get(1).unwrap().version, 2);
        assert_eq!(history.get(2).unwrap().version, 3);
    }

    #[test]
    fn test_history_preserves_field_values_per_version() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        client.set_anchor_metadata(&anchor, &1000u32, &600u64, &1000u32, &9000u32, &100_000u64);
        set_ledger(&env, 2_000);
        client.set_anchor_metadata(&anchor, &9999u32, &100u64, &9999u32, &9999u32, &999_999u64);

        let history = client.get_anchor_metadata_history(&anchor, &10u32);
        assert_eq!(history.len(), 2);

        let v1 = history.get(0).unwrap();
        assert_eq!(v1.metadata.reputation_score, 1000);
        assert_eq!(v1.metadata.average_settlement_time, 600);
        assert_eq!(v1.updated_at, 1_000);

        let v2 = history.get(1).unwrap();
        assert_eq!(v2.metadata.reputation_score, 9999);
        assert_eq!(v2.metadata.average_settlement_time, 100);
        assert_eq!(v2.updated_at, 2_000);
    }

    #[test]
    fn test_history_limit_respected() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        for i in 0u64..10 {
            set_ledger(&env, 1_000 + i * 100);
            client.set_anchor_metadata(
                &anchor,
                &(i as u32 * 1000),
                &(600 - i * 50),
                &7000u32,
                &9000u32,
                &(i * 100_000),
            );
        }

        // Request only the last 3 versions
        let history = client.get_anchor_metadata_history(&anchor, &3u32);
        assert_eq!(history.len(), 3);
        // The last 3 versions should be 8, 9, 10
        assert_eq!(history.get(0).unwrap().version, 8);
        assert_eq!(history.get(2).unwrap().version, 10);
    }

    #[test]
    fn test_history_empty_for_unknown_anchor() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        let history = client.get_anchor_metadata_history(&anchor, &10u32);
        assert_eq!(history.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Point-in-time version lookup tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_metadata_at_version_returns_correct_snapshot() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        client.set_anchor_metadata(&anchor, &1111u32, &111u64, &1111u32, &9000u32, &111_111u64);
        set_ledger(&env, 2_000);
        client.set_anchor_metadata(&anchor, &2222u32, &222u64, &2222u32, &9200u32, &222_222u64);

        let v1 = client.get_anchor_metadata_at_version(&anchor, &1u32);
        assert_eq!(v1.version, 1);
        assert_eq!(v1.metadata.reputation_score, 1111);
        assert_eq!(v1.updated_at, 1_000);

        let v2 = client.get_anchor_metadata_at_version(&anchor, &2u32);
        assert_eq!(v2.version, 2);
        assert_eq!(v2.metadata.reputation_score, 2222);
        assert_eq!(v2.updated_at, 2_000);
    }

    #[test]
    fn test_get_metadata_at_version_nonexistent_panics() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        // No metadata set — version 1 should not exist
        let result = client.try_get_anchor_metadata_at_version(&anchor, &1u32);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_metadata_at_version_zero_panics() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        client.set_anchor_metadata(&anchor, &8000u32, &300u64, &7500u32, &9900u32, &1_000_000u64);

        // Version 0 is invalid
        let result = client.try_get_anchor_metadata_at_version(&anchor, &0u32);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Rollback semantics test
    // -----------------------------------------------------------------------

    #[test]
    fn test_rollback_semantics_via_history() {
        // Demonstrates that a caller can "roll back" by reading a prior version
        // and re-applying it as the current metadata.
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        // v1: good state
        client.set_anchor_metadata(&anchor, &8000u32, &300u64, &7500u32, &9900u32, &1_000_000u64);
        // v2: bad update (e.g., wrong reputation)
        set_ledger(&env, 2_000);
        client.set_anchor_metadata(&anchor, &1u32, &9999u64, &1u32, &1u32, &1u64);

        // Current metadata reflects v2
        let current = client.get_anchor_metadata(&anchor);
        assert_eq!(current.reputation_score, 1);

        // Retrieve v1 snapshot and re-apply it (simulating rollback)
        let v1 = client.get_anchor_metadata_at_version(&anchor, &1u32);
        set_ledger(&env, 3_000);
        client.set_anchor_metadata(
            &anchor,
            &v1.metadata.reputation_score,
            &v1.metadata.average_settlement_time,
            &v1.metadata.liquidity_score,
            &v1.metadata.uptime_percentage,
            &v1.metadata.total_volume,
        );

        // Now current metadata matches the rolled-back v1 values
        let restored = client.get_anchor_metadata(&anchor);
        assert_eq!(restored.reputation_score, 8000);
        assert_eq!(restored.average_settlement_time, 300);

        // Version count is now 3 (v1, v2, v3=rollback)
        let count = client.get_anchor_meta_version_count(&anchor);
        assert_eq!(count, 3);
    }

    // -----------------------------------------------------------------------
    // Independent anchors don't share history
    // -----------------------------------------------------------------------

    #[test]
    fn test_history_is_per_anchor() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let anchor_a = Address::generate(&env);
        let anchor_b = Address::generate(&env);

        client.set_anchor_metadata(&anchor_a, &1000u32, &100u64, &1000u32, &9000u32, &100_000u64);
        client.set_anchor_metadata(&anchor_a, &2000u32, &200u64, &2000u32, &9100u32, &200_000u64);
        client.set_anchor_metadata(&anchor_b, &5000u32, &500u64, &5000u32, &9500u32, &500_000u64);

        assert_eq!(client.get_anchor_meta_version_count(&anchor_a), 2);
        assert_eq!(client.get_anchor_meta_version_count(&anchor_b), 1);

        let hist_b = client.get_anchor_metadata_history(&anchor_b, &10u32);
        assert_eq!(hist_b.len(), 1);
        assert_eq!(hist_b.get(0).unwrap().metadata.reputation_score, 5000);
    }
}
