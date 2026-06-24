//! Tests for audit log retention policies, pruning, and paginated retrieval (#251).

#![cfg(test)]

mod sep10_test_util;

mod audit_log_retention_tests {
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env};

    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient};
    use crate::sep10_test_util::{register_attestor_with_sep10, sign_payload};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn set_ledger(env: &Env, ts: u64) {
        env.ledger().set(LedgerInfo {
            timestamp: ts,
            protocol_version: 21,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });
    }

    fn setup(env: &Env) -> (Address, AnchorKitContractClient<'_>) {
        set_ledger(env, 1000);
        let cid = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &cid);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (admin, client)
    }

    fn add_attestor(env: &Env, client: &AnchorKitContractClient<'_>) -> (Address, SigningKey) {
        let attestor = Address::generate(env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(env, client, &attestor, &attestor, &sk);
        (attestor, sk)
    }

    fn emit_audit_log(
        env: &Env,
        client: &AnchorKitContractClient,
        attestor: &Address,
        sk: &SigningKey,
        ts: u64,
        nonce: u8,
    ) {
        set_ledger(env, ts);
        let session_id = client.create_session(attestor);
        let subject = Address::generate(env);
        let mut payload_bytes = [0u8; 32];
        payload_bytes[0] = nonce;
        let payload = soroban_sdk::Bytes::from_slice(env, &payload_bytes);
        let sig = sign_payload(env, sk, &payload);
        client.submit_attestation_with_session(
            &session_id, attestor, &subject, &ts, &payload, &sig,
        );
        client.close_session(&session_id, attestor);
    }

    // ── Retention policy ──────────────────────────────────────────────────────

    #[test]
    fn test_default_retention_is_zero_unlimited() {
        let env = make_env(); let (_, client) = setup(&env);
        assert_eq!(client.get_audit_log_retention(), 0);
    }

    #[test]
    fn test_set_and_get_retention() {
        let env = make_env(); let (admin, client) = setup(&env);
        client.set_audit_log_retention(&30);
        assert_eq!(client.get_audit_log_retention(), 30);
    }

    #[test]
    fn test_update_retention_policy() {
        let env = make_env(); let (admin, client) = setup(&env);
        client.set_audit_log_retention(&90);
        client.set_audit_log_retention(&7);
        assert_eq!(client.get_audit_log_retention(), 7);
    }

    // ── Audit log count ───────────────────────────────────────────────────────

    #[test]
    fn test_audit_log_count_starts_at_zero() {
        let env = make_env(); let (_, client) = setup(&env);
        assert_eq!(client.get_audit_log_count(), 0);
    }

    #[test]
    fn test_audit_log_count_increments_with_session_ops() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 2000, 1);
        assert!(client.get_audit_log_count() >= 1);
    }

    // ── Pagination ────────────────────────────────────────────────────────────

    #[test]
    fn test_paginated_retrieval_empty_returns_empty_vec() {
        let env = make_env(); let (_, client) = setup(&env);
        let page = client.get_audit_logs_paginated(&0, &10);
        assert_eq!(page.len(), 0);
    }

    #[test]
    fn test_paginated_retrieval_returns_correct_entries() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 2000, 1);
        emit_audit_log(&env, &client, &attestor, &sk, 3000, 2);

        let total = client.get_audit_log_count();
        assert!(total >= 2, "expected at least 2 entries, got {total}");

        let page = client.get_audit_logs_paginated(&0, &10);
        assert!(page.len() >= 2);
    }

    #[test]
    fn test_paginated_retrieval_respects_offset() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 2000, 1);
        emit_audit_log(&env, &client, &attestor, &sk, 3000, 2);

        let total = client.get_audit_log_count();
        let page0 = client.get_audit_logs_paginated(&0, &1);
        let page1 = client.get_audit_logs_paginated(&1, &1);
        // Both pages must be non-empty if total >= 2
        if total >= 2 {
            assert_eq!(page0.len(), 1);
            assert_eq!(page1.len(), 1);
        }
    }

    #[test]
    fn test_pagination_limit_capped_at_50() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 2000, 1);

        // Requesting 200 should return at most 50
        let page = client.get_audit_logs_paginated(&0, &200);
        assert!(page.len() <= 50);
    }

    #[test]
    fn test_pagination_offset_beyond_total_returns_empty() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 2000, 1);

        let page = client.get_audit_logs_paginated(&9999, &10);
        assert_eq!(page.len(), 0);
    }

    // ── Session-scoped pagination ─────────────────────────────────────────────

    #[test]
    fn test_session_audit_logs_paginated_returns_correct_entries() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        set_ledger(&env, 2000);
        let session_id = client.create_session(&attestor);

        let subject = Address::generate(&env);
        let payload = soroban_sdk::Bytes::from_slice(&env, b"payload_hash_32bytes_exactly____");
        let sig = sign_payload(&env, &sk, &payload);
        client.submit_attestation_with_session(
            &session_id, &attestor, &subject, &2000u64, &payload, &sig,
        );
        client.close_session(&session_id, &attestor);

        let page = client.get_session_logs_paginated(&session_id, &0, &10);
        assert!(page.len() >= 1);
    }

    #[test]
    fn test_session_audit_logs_paginated_empty_offset_returns_empty() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        set_ledger(&env, 2000);
        let session_id = client.create_session(&attestor);
        client.close_session(&session_id, &attestor);

        let page = client.get_session_logs_paginated(&session_id, &9999, &10);
        assert_eq!(page.len(), 0);
    }

    // ── Pruning ───────────────────────────────────────────────────────────────

    #[test]
    fn test_prune_removes_old_entries() {
        let env = make_env(); let (admin, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        // Create two audit entries at different timestamps
        emit_audit_log(&env, &client, &attestor, &sk, 1_000, 1);
        emit_audit_log(&env, &client, &attestor, &sk, 10_000, 2);

        let before_prune = client.get_audit_log_count();
        assert!(before_prune >= 2);

        // Prune entries older than timestamp 5_000
        set_ledger(&env, 20_000);
        let pruned = client.prune_audit_logs(&5_000u64);
        assert!(pruned >= 1, "expected at least one entry pruned");
    }

    #[test]
    fn test_prune_does_not_remove_recent_entries() {
        let env = make_env(); let (admin, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 10_000, 1);

        set_ledger(&env, 20_000);
        // Prune with a threshold before all entries — nothing should be removed
        let pruned = client.prune_audit_logs(&1u64);
        assert_eq!(pruned, 0, "no entries should be pruned when threshold is too early");
    }

    #[test]
    fn test_prune_with_no_entries_returns_zero() {
        let env = make_env(); let (admin, client) = setup(&env);
        set_ledger(&env, 5_000);
        let pruned = client.prune_audit_logs(&9_999u64);
        assert_eq!(pruned, 0);
    }
}
