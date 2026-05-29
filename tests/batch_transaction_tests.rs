#![cfg(test)]

mod batch_transaction_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Env,
    };

    use crate::contract::{AnchorKitContract, AnchorKitContractClient};
    use crate::transaction_state_tracker::{TransactionState, TransactionStateTracker};

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
    // Contract-level batch query tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_transactions_in_range_basic() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let initiator = Address::generate(&env);

        // Create 5 transaction records with IDs 1..=5
        for id in 1u64..=5 {
            client.create_transaction_record(&id, &initiator);
        }

        let result = client.get_transactions_in_range(&1u64, &5u64, &5u32);
        assert_eq!(result.total, 5);
        assert_eq!(result.records.len(), 5);
    }

    #[test]
    fn test_get_transactions_in_range_partial_window() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let initiator = Address::generate(&env);

        for id in 1u64..=10 {
            client.create_transaction_record(&id, &initiator);
        }

        // Request IDs 3..=7 with limit 3 — should return 3 records, total=5
        let result = client.get_transactions_in_range(&3u64, &7u64, &3u32);
        assert_eq!(result.total, 5);
        assert_eq!(result.records.len(), 3);
        // First record in the page must be ID 3
        assert_eq!(result.records.get(0).unwrap().transaction_id, 3);
    }

    #[test]
    fn test_get_transactions_in_range_sparse_ids() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let initiator = Address::generate(&env);

        // Only create records for IDs 2 and 4 — IDs 1, 3, 5 are absent
        client.create_transaction_record(&2u64, &initiator);
        client.create_transaction_record(&4u64, &initiator);

        let result = client.get_transactions_in_range(&1u64, &5u64, &10u32);
        assert_eq!(result.total, 2);
        assert_eq!(result.records.len(), 2);
    }

    #[test]
    fn test_get_transactions_in_range_limit_capped_at_100() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let initiator = Address::generate(&env);

        for id in 1u64..=10 {
            client.create_transaction_record(&id, &initiator);
        }

        // Passing limit=0 should default to the internal cap (100), returning all 10
        let result = client.get_transactions_in_range(&1u64, &10u64, &0u32);
        assert_eq!(result.total, 10);
        assert_eq!(result.records.len(), 10);
    }

    #[test]
    fn test_get_transactions_in_range_empty_range() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);

        // No records created — range should return empty
        let result = client.get_transactions_in_range(&100u64, &200u64, &50u32);
        assert_eq!(result.total, 0);
        assert_eq!(result.records.len(), 0);
    }

    #[test]
    fn test_summarize_transactions_by_status_mixed_states() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let initiator = Address::generate(&env);

        // IDs 1-4: create all as Pending
        for id in 1u64..=4 {
            client.create_transaction_record(&id, &initiator);
        }

        // The contract's create_transaction_record only creates Pending records.
        // Use the tracker directly to advance states for the summary test.
        // We verify the summary via the tracker's summarize_by_status method.
        let mut tracker = TransactionStateTracker::new(true);
        let env2 = Env::default();
        let init2 = Address::generate(&env2);

        tracker.create_transaction(1, init2.clone(), &env2).unwrap();
        tracker.create_transaction(2, init2.clone(), &env2).unwrap();
        tracker.create_transaction(3, init2.clone(), &env2).unwrap();
        tracker.create_transaction(4, init2.clone(), &env2).unwrap();

        tracker.start_transaction(1, &env2).unwrap();
        tracker.start_transaction(2, &env2).unwrap();
        tracker.complete_transaction(2, &env2).unwrap();
        tracker.start_transaction(3, &env2).unwrap();
        tracker
            .fail_transaction(3, soroban_sdk::String::from_str(&env2, "err"), &env2)
            .unwrap();

        // State: 1=InProgress, 2=Completed, 3=Failed, 4=Pending
        let (pending, in_progress, completed, failed) =
            tracker.summarize_by_status(1, 4, &env2).unwrap();
        assert_eq!(pending, 1);
        assert_eq!(in_progress, 1);
        assert_eq!(completed, 1);
        assert_eq!(failed, 1);
    }

    #[test]
    fn test_summarize_transactions_by_status_all_pending() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        let initiator = Address::generate(&env);

        for id in 1u64..=5 {
            client.create_transaction_record(&id, &initiator);
        }

        let summary = client.summarize_transactions_by_status(&1u64, &5u64);
        assert_eq!(summary.pending_count, 5);
        assert_eq!(summary.in_progress_count, 0);
        assert_eq!(summary.completed_count, 0);
        assert_eq!(summary.failed_count, 0);
        assert_eq!(summary.total_count, 5);
    }

    #[test]
    fn test_summarize_transactions_by_status_empty() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);

        let summary = client.summarize_transactions_by_status(&50u64, &100u64);
        assert_eq!(summary.total_count, 0);
    }

    // -----------------------------------------------------------------------
    // TransactionStateTracker unit-level batch tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_tracker_get_transactions_in_range_dev_mode() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for id in 1u64..=6 {
            tracker.create_transaction(id, initiator.clone(), &env).unwrap();
        }

        let results = tracker.get_transactions_in_range(2, 5, 10, &env).unwrap();
        assert_eq!(results.len(), 4);
        // Verify ascending order
        assert_eq!(results[0].transaction_id, 2);
        assert_eq!(results[3].transaction_id, 5);
    }

    #[test]
    fn test_tracker_get_transactions_in_range_respects_limit() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for id in 1u64..=10 {
            tracker.create_transaction(id, initiator.clone(), &env).unwrap();
        }

        let results = tracker.get_transactions_in_range(1, 10, 3, &env).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_tracker_get_transactions_in_range_invalid_range() {
        let env = Env::default();
        let tracker = TransactionStateTracker::new(true);

        let result = tracker.get_transactions_in_range(10, 5, 10, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_tracker_summarize_by_status_dev_mode() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for id in 1u64..=5 {
            tracker.create_transaction(id, initiator.clone(), &env).unwrap();
        }
        tracker.start_transaction(1, &env).unwrap();
        tracker.complete_transaction(1, &env).unwrap();
        tracker.start_transaction(2, &env).unwrap();
        tracker
            .fail_transaction(2, soroban_sdk::String::from_str(&env, "fail"), &env)
            .unwrap();

        // IDs 1-5: 1=Completed, 2=Failed, 3-5=Pending
        let (pending, in_progress, completed, failed) =
            tracker.summarize_by_status(1, 5, &env).unwrap();
        assert_eq!(pending, 3);
        assert_eq!(in_progress, 0);
        assert_eq!(completed, 1);
        assert_eq!(failed, 1);
    }

    #[test]
    fn test_tracker_summarize_by_status_invalid_range() {
        let env = Env::default();
        let tracker = TransactionStateTracker::new(true);

        let result = tracker.summarize_by_status(10, 5, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_tracker_summarize_excludes_out_of_range_ids() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        // IDs 1, 5, 10
        tracker.create_transaction(1, initiator.clone(), &env).unwrap();
        tracker.create_transaction(5, initiator.clone(), &env).unwrap();
        tracker.create_transaction(10, initiator.clone(), &env).unwrap();

        // Summarize only IDs 1..=5 — should count 2, not 3
        let (pending, _, _, _) = tracker.summarize_by_status(1, 5, &env).unwrap();
        assert_eq!(pending, 2);
    }
}
