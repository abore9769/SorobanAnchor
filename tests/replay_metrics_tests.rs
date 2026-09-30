#![cfg(test)]

mod sep10_test_util;

mod replay_metrics_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Bytes, Env,
    };
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient};
    use crate::sep10_test_util::{register_attestor_with_sep10, sign_payload};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1_000_000,
            protocol_version: 21,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6312000,
        });
        env
    }

    fn payload(env: &Env, byte: u8) -> Bytes {
        let mut b = Bytes::new(env);
        for _ in 0..32 {
            b.push_back(byte);
        }
        b
    }

    /// Set up a fresh contract with one registered attestor.
    fn setup(env: &Env) -> (AnchorKitContractClient, Address, Address, SigningKey) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let issuer = Address::generate(env);
        client.initialize(&admin);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(env, &client, &issuer, &issuer, &sk);
        (client, admin, issuer, sk)
    }

    // -----------------------------------------------------------------------
    // Accepted-event counter increments after a successful attestation
    // -----------------------------------------------------------------------

    #[test]
    fn test_accepted_events_counter_increments_on_submit() {
        let env = make_env();
        let (client, _, issuer, sk) = setup(&env);

        let hash = payload(&env, 0x01);
        let sig = sign_payload(&env, &sk, &hash);
        let subject = Address::generate(&env);

        // Before any submission the counter must be zero.
        let before = client.get_replay_metrics();
        assert_eq!(before.accepted_events, 0);

        client.submit_attestation(&issuer, &subject, &1_000_001u64, &hash, &sig);

        let after = client.get_replay_metrics();
        assert_eq!(after.accepted_events, 1);
        assert_eq!(after.total_replay_attempts, 0);
        assert_eq!(after.skipped_events, 0);
    }

    #[test]
    fn test_accepted_events_accumulate_across_multiple_submissions() {
        let env = make_env();
        let (client, _, issuer, sk) = setup(&env);
        let subject = Address::generate(&env);

        for i in 0u8..3 {
            let hash = payload(&env, i + 1);
            let sig = sign_payload(&env, &sk, &hash);
            client.submit_attestation(&issuer, &subject, &(1_000_001u64 + i as u64), &hash, &sig);
        }

        let metrics = client.get_replay_metrics();
        assert_eq!(metrics.accepted_events, 3);
        assert_eq!(metrics.total_replay_attempts, 0);
    }

    // -----------------------------------------------------------------------
    // Replay-attempt counter increments on duplicate, accepted stays unchanged
    // -----------------------------------------------------------------------

    #[test]
    fn test_replay_attempt_does_not_increment_accepted() {
        let env = make_env();
        let (client, _, issuer, sk) = setup(&env);
        let subject = Address::generate(&env);

        let hash = payload(&env, 0xAA);
        let sig = sign_payload(&env, &sk, &hash);

        // First submission — should succeed and bump accepted.
        client.submit_attestation(&issuer, &subject, &1_000_001u64, &hash, &sig);

        let mid = client.get_replay_metrics();
        assert_eq!(mid.accepted_events, 1);
        assert_eq!(mid.total_replay_attempts, 0);

        // Second submission with same hash — should be rejected as a replay.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.submit_attestation(&issuer, &subject, &1_000_002u64, &hash, &sig);
        }));
        assert!(result.is_err(), "duplicate submission must be rejected");

        let after = client.get_replay_metrics();
        // accepted must NOT have changed.
        assert_eq!(after.accepted_events, 1, "accepted_events must not change on replay");
        // replay counter must have incremented.
        assert_eq!(after.total_replay_attempts, 1);
        assert_eq!(after.unique_replayed_ids, 1);
    }

    // -----------------------------------------------------------------------
    // Counters are independent across different issuers
    // -----------------------------------------------------------------------

    #[test]
    fn test_accepted_events_count_multiple_issuers() {
        let env = make_env();
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let issuer_a = Address::generate(&env);
        let issuer_b = Address::generate(&env);
        let subject = Address::generate(&env);

        let sk_a = SigningKey::generate(&mut OsRng);
        let sk_b = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &issuer_a, &issuer_a, &sk_a);
        register_attestor_with_sep10(&env, &client, &issuer_b, &issuer_b, &sk_b);

        // Same hash, two different issuers — both should be accepted.
        let hash = payload(&env, 0x42);
        let sig_a = sign_payload(&env, &sk_a, &hash);
        let sig_b = sign_payload(&env, &sk_b, &hash);
        client.submit_attestation(&issuer_a, &subject, &1_000_001u64, &hash, &sig_a);
        client.submit_attestation(&issuer_b, &subject, &1_000_002u64, &hash, &sig_b);

        let metrics = client.get_replay_metrics();
        assert_eq!(metrics.accepted_events, 2);
        assert_eq!(metrics.total_replay_attempts, 0);
    }

    // -----------------------------------------------------------------------
    // get_replay_count_for_id reflects per-id replay count
    // -----------------------------------------------------------------------

    #[test]
    fn test_replay_count_per_id_matches_total_attempts() {
        let env = make_env();
        let (client, _, issuer, sk) = setup(&env);
        let subject = Address::generate(&env);

        let hash = payload(&env, 0x77);
        let sig = sign_payload(&env, &sk, &hash);

        client.submit_attestation(&issuer, &subject, &1_000_001u64, &hash, &sig);

        // First replay
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.submit_attestation(&issuer, &subject, &1_000_002u64, &hash, &sig);
        }));
        // Second replay
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.submit_attestation(&issuer, &subject, &1_000_003u64, &hash, &sig);
        }));

        let metrics = client.get_replay_metrics();
        assert_eq!(metrics.total_replay_attempts, 2);
        assert_eq!(metrics.unique_replayed_ids, 1);

        let per_id = client.get_replay_count_for_id(&hash);
        assert_eq!(per_id, 2, "per-id count must match replay attempts for that id");
    }

    // -----------------------------------------------------------------------
    // Overflow protection — counters pinned at u64::MAX must saturate
    // -----------------------------------------------------------------------

    /// Aggregate metrics live in instance storage and can be restored from an
    /// external snapshot at `u64::MAX`. Plain `+= 1` would wrap them to `0`,
    /// silently erasing the recorded history; the module policy is saturation
    /// (as already used by the per-ID attempt counter).
    #[test]
    fn test_replay_metrics_saturate_at_u64_max() {
        use anchorkit::replay_detection::{record_accepted_event, record_replay_detection, record_skipped_event, ReplayMetrics};
        use soroban_sdk::symbol_short;

        let env = make_env();
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let at_max = ReplayMetrics {
            total_replay_attempts: u64::MAX,
            unique_replayed_ids: u64::MAX,
            last_replay_at: 0,
            last_updated_ledger: 0,
            accepted_events: u64::MAX,
            skipped_events: u64::MAX,
        };

        env.as_contract(&contract_id, || {
            env.storage().instance().set(&symbol_short!("REPLAYM"), &at_max);
        });

        // Record a replay attempt against the saturated metrics.
        let request_id = payload(&env, 0x9C);
        let actor = Address::generate(&env);
        env.as_contract(&contract_id, || {
            record_replay_detection(&env, &request_id, &actor);
        });

        let m = client.get_replay_metrics();
        assert_eq!(m.total_replay_attempts, u64::MAX, "total_replay_attempts must saturate, not wrap to 0");
        assert_eq!(m.unique_replayed_ids, u64::MAX, "unique_replayed_ids must saturate, not wrap to 0");

        // Accepted / skipped share the same metrics record.
        env.as_contract(&contract_id, || {
            record_accepted_event(&env);
            record_skipped_event(&env);
        });

        let m = client.get_replay_metrics();
        assert_eq!(m.accepted_events, u64::MAX, "accepted_events must saturate, not wrap to 0");
        assert_eq!(m.skipped_events, u64::MAX, "skipped_events must saturate, not wrap to 0");
    }

    /// Saturation must not disturb ordinary increments or replay decisions.
    #[test]
    fn test_replay_metrics_normal_increments_unchanged() {
        use anchorkit::replay_detection::{get_replay_metrics, record_replay_detection, ReplayMetrics};
        use soroban_sdk::symbol_short;

        let env = make_env();
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // One below maximum: a single increment must land exactly on MAX.
        let near_max = ReplayMetrics {
            total_replay_attempts: u64::MAX - 1,
            unique_replayed_ids: 0,
            last_replay_at: 0,
            last_updated_ledger: 0,
            accepted_events: 7,
            skipped_events: 11,
        };
        env.as_contract(&contract_id, || {
            env.storage().instance().set(&symbol_short!("REPLAYM"), &near_max);
        });

        let request_id = payload(&env, 0x2B);
        let actor = Address::generate(&env);
        env.as_contract(&contract_id, || {
            record_replay_detection(&env, &request_id, &actor);
        });

        let m = env.as_contract(&contract_id, || get_replay_metrics(&env));
        assert_eq!(m.total_replay_attempts, u64::MAX, "MAX - 1 + 1 must be exactly MAX");
        assert_eq!(m.unique_replayed_ids, 1, "first attempt on this id still counts as unique");
        assert_eq!(m.accepted_events, 7, "accepted_events must be untouched by a replay record");
        assert_eq!(m.skipped_events, 11, "skipped_events must be untouched by a replay record");
        assert_eq!(client.get_replay_count_for_id(&request_id), 1, "replay decision must still be recorded");
    }
}
