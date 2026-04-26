/// Transaction State Tracker Tests
/// This test file demonstrates and validates the Transaction State Tracker implementation

use crate::transaction_state_tracker::*;
use soroban_sdk::Env;

#[cfg(test)]
mod transaction_state_tracker_tests {
    use super::*;
    use soroban_sdk::testutils::Address;
    use soroban_sdk::String;

    #[test]
    fn test_transaction_state_to_string() {
        assert_eq!(TransactionState::Pending.as_str(), "pending");
        assert_eq!(TransactionState::InProgress.as_str(), "in_progress");
        assert_eq!(TransactionState::Completed.as_str(), "completed");
        assert_eq!(TransactionState::Failed.as_str(), "failed");
    }

    #[test]
    fn test_transaction_state_from_string() {
        assert_eq!(
            TransactionState::from_str("pending"),
            Some(TransactionState::Pending)
        );
        assert_eq!(
            TransactionState::from_str("in_progress"),
            Some(TransactionState::InProgress)
        );
        assert_eq!(
            TransactionState::from_str("completed"),
            Some(TransactionState::Completed)
        );
        assert_eq!(
            TransactionState::from_str("failed"),
            Some(TransactionState::Failed)
        );
        assert_eq!(TransactionState::from_str("unknown"), None);
    }

    #[test]
    fn test_full_transaction_lifecycle() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        // Create transaction -> Pending
        let tx1_result = tracker.create_transaction(1, initiator.clone(), &env);
        assert!(tx1_result.is_ok());
        let tx1 = tx1_result.unwrap();
        assert_eq!(tx1.state, TransactionState::Pending);

        // Start transaction -> In-progress
        let tx2_result = tracker.start_transaction(1, &env);
        assert!(tx2_result.is_ok());
        let tx2 = tx2_result.unwrap();
        assert_eq!(tx2.state, TransactionState::InProgress);

        // Complete transaction -> Completed
        let tx3_result = tracker.complete_transaction(1, &env);
        assert!(tx3_result.is_ok());
        let tx3 = tx3_result.unwrap();
        assert_eq!(tx3.state, TransactionState::Completed);
    }

    #[test]
    fn test_transaction_failure_with_error_message() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();

        let error_msg = String::from_str(&env, "Payment declined");
        let result = tracker.fail_transaction(1, error_msg.clone(), &env);

        assert!(result.is_ok());
        let record = result.unwrap();
        assert_eq!(record.state, TransactionState::Failed);
        assert_eq!(record.error_message, Some(error_msg));
    }

    #[test]
    fn test_query_transactions_by_state() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        // Create 5 transactions
        for i in 1..=5 {
            tracker.create_transaction(i, initiator.clone(), &env).ok();
        }

        // Move some to in-progress
        tracker.start_transaction(1, &env).ok();
        tracker.start_transaction(2, &env).ok();

        // Complete one
        tracker.complete_transaction(2, &env).ok();

        // Query by state
        let pending_result = tracker.get_transactions_by_state(TransactionState::Pending);
        assert!(pending_result.is_ok());
        let pending = pending_result.unwrap();
        assert_eq!(pending.len(), 3); // 3, 4, 5

        let in_progress_result = tracker.get_transactions_by_state(TransactionState::InProgress);
        assert!(in_progress_result.is_ok());
        let in_progress = in_progress_result.unwrap();
        assert_eq!(in_progress.len(), 1); // 1

        let completed_result = tracker.get_transactions_by_state(TransactionState::Completed);
        assert!(completed_result.is_ok());
        let completed = completed_result.unwrap();
        assert_eq!(completed.len(), 1); // 2
    }

    #[test]
    fn test_production_mode_flag() {
        let env = Env::default();
        let mut prod_tracker = TransactionStateTracker::new(false);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        // In production mode, cache should not be populated
        let result = prod_tracker.create_transaction(1, initiator.clone(), &env);
        assert!(result.is_ok());
        assert_eq!(prod_tracker.cache_size(), 0); // Should be 0 in production mode

        // In dev mode, cache should be populated
        let mut dev_tracker = TransactionStateTracker::new(true);
        let result = dev_tracker.create_transaction(1, initiator.clone(), &env);
        assert!(result.is_ok());
        assert_eq!(dev_tracker.cache_size(), 1); // Should be 1 in dev mode
    }

    #[test]
    fn test_transaction_not_found() {
        let env = Env::default();
        let tracker = TransactionStateTracker::new(true);

        let result = tracker.get_transaction_state(999, &env);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_multiple_transactions_isolation() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator1 = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let initiator2 = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        // Create transactions from different initiators
        tracker.create_transaction(1, initiator1.clone(), &env).ok();
        tracker.create_transaction(2, initiator2.clone(), &env).ok();

        // Update first one
        tracker.start_transaction(1, &env).ok();

        // Verify second one is still pending
        let tx2_state = tracker.get_transaction_state(2, &env);
        assert!(tx2_state.is_ok());
        let tx2 = tx2_state.unwrap().unwrap();
        assert_eq!(tx2.state, TransactionState::Pending);
        assert_eq!(tx2.initiator, initiator2);
    }

    #[test]
    fn test_clear_cache_dev_mode() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.create_transaction(2, initiator.clone(), &env).ok();
        assert_eq!(tracker.cache_size(), 2);

        let clear_result = tracker.clear_cache();
        assert!(clear_result.is_ok());
        assert_eq!(tracker.cache_size(), 0);
    }

    #[test]
    fn test_timestamp_tracking() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        let create_result = tracker.create_transaction(1, initiator.clone(), &env);
        let record1 = create_result.unwrap();
        let initial_timestamp = record1.timestamp;

        let update_result = tracker.start_transaction(1, &env);
        let record2 = update_result.unwrap();

        // Timestamps should be set and last_updated should reflect the change
        assert_eq!(record2.timestamp, initial_timestamp);
        assert!(record2.last_updated >= initial_timestamp);
    }

    // -----------------------------------------------------------------------
    // Backward / same-state transition guard
    // -----------------------------------------------------------------------

    #[test]
    fn test_illegal_backward_transition_rejected() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        tracker.complete_transaction(1, &env).ok();

        // Completed → Pending is illegal
        let r = tracker.advance_transaction_state(1, TransactionState::Pending, &env);
        assert!(r.is_err());

        // Completed → InProgress is illegal
        let r = tracker.advance_transaction_state(1, TransactionState::InProgress, &env);
        assert!(r.is_err());
    }

    #[test]
    fn test_illegal_same_state_transition_rejected() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();

        // Pending → Pending is not a valid transition
        let r = tracker.advance_transaction_state(1, TransactionState::Pending, &env);
        assert!(r.is_err());
    }

    #[test]
    fn test_valid_forward_transition_accepted() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();

        let r = tracker.advance_transaction_state(1, TransactionState::InProgress, &env);
        assert!(r.is_ok());
        assert_eq!(r.unwrap().state, TransactionState::InProgress);

        let r = tracker.advance_transaction_state(1, TransactionState::Completed, &env);
        assert!(r.is_ok());
        assert_eq!(r.unwrap().state, TransactionState::Completed);
    }

    // -----------------------------------------------------------------------
    // Audit log entries for success and failure
    // -----------------------------------------------------------------------

    #[test]
    fn test_audit_log_entry_created_on_successful_transition() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();

        assert_eq!(tracker.audit_log.len(), 1);
        let entry = &tracker.audit_log[0];
        assert_eq!(entry.from_state, TransactionState::Pending);
        assert_eq!(entry.to_state, TransactionState::InProgress);
        assert!(entry.success);
    }

    #[test]
    fn test_audit_log_entry_created_on_failed_transition() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        // Illegal: Pending → Completed
        let _ = tracker.advance_transaction_state(1, TransactionState::Completed, &env);

        assert_eq!(tracker.audit_log.len(), 1);
        let entry = &tracker.audit_log[0];
        assert_eq!(entry.from_state, TransactionState::Pending);
        assert_eq!(entry.to_state, TransactionState::Completed);
        assert!(!entry.success);
    }
}

#[cfg(test)]
mod snapshot_tests {
    use std::collections::HashMap;

    /// Minimal snapshot representation matching the JSON fixtures.
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct RecordSnapshot {
        transaction_id: u64,
        state: String,
        state_u32: u32,
        initiator: String,
        timestamp: u64,
        last_updated: u64,
        error_message: Option<String>,
    }

    fn load_snapshot(name: &str) -> RecordSnapshot {
        let path = format!(
            "{}/test_snapshots/transaction_state_tracker_tests/{}.json",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let data = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("missing snapshot: {path}"));
        serde_json::from_str(&data).unwrap_or_else(|e| panic!("bad snapshot {name}: {e}"))
    }

    #[test]
    fn snapshot_state_discriminants() {
        let cases: HashMap<&str, (&str, u32)> = [
            ("record_pending",     ("Pending",    1)),
            ("record_in_progress", ("InProgress", 2)),
            ("record_completed",   ("Completed",  3)),
            ("record_failed",      ("Failed",     4)),
        ]
        .into();

        for (file, (expected_state, expected_u32)) in &cases {
            let snap = load_snapshot(file);
            assert_eq!(
                snap.state, *expected_state,
                "{file}: state name changed — on-chain encoding regression"
            );
            assert_eq!(
                snap.state_u32, *expected_u32,
                "{file}: state discriminant changed — on-chain encoding regression"
            );
        }
    }

    #[test]
    fn snapshot_failed_has_error_message() {
        let snap = load_snapshot("record_failed");
        assert!(
            snap.error_message.is_some(),
            "record_failed snapshot must have an error_message"
        );
    }

    #[test]
    fn snapshot_non_failed_no_error_message() {
        for name in &["record_pending", "record_in_progress", "record_completed"] {
            let snap = load_snapshot(name);
            assert!(
                snap.error_message.is_none(),
                "{name} snapshot must not have an error_message"
            );
        }
    }
}
