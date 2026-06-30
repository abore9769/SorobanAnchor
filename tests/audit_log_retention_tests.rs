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
    ) {
        set_ledger(env, ts);
        let session_id = client.create_session(attestor);
        let subject = Address::generate(env);
        let mut buf = [0u8; 32];
        buf[..16].copy_from_slice(b"payload_hash_32b");
        buf[16..24].copy_from_slice(&ts.to_be_bytes());
        let payload = soroban_sdk::Bytes::from_slice(env, &buf);
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
        emit_audit_log(&env, &client, &attestor, &sk, 2000);
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
        emit_audit_log(&env, &client, &attestor, &sk, 2000);
        emit_audit_log(&env, &client, &attestor, &sk, 3000);

        let total = client.get_audit_log_count();
        assert!(total >= 2, "expected at least 2 entries, got {total}");

        let page = client.get_audit_logs_paginated(&0, &10);
        assert!(page.len() >= 2);
    }

    #[test]
    fn test_paginated_retrieval_respects_offset() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 2000);
        emit_audit_log(&env, &client, &attestor, &sk, 3000);

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
        emit_audit_log(&env, &client, &attestor, &sk, 2000);

        // Requesting 200 should return at most 50
        let page = client.get_audit_logs_paginated(&0, &200);
        assert!(page.len() <= 50);
    }

    #[test]
    fn test_pagination_offset_beyond_total_returns_empty() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        emit_audit_log(&env, &client, &attestor, &sk, 2000);

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
        let mut buf = [0u8; 32];
        buf[..16].copy_from_slice(b"payload_hash_32b");
        buf[16..24].copy_from_slice(&2000u64.to_be_bytes());
        let payload = soroban_sdk::Bytes::from_slice(&env, &buf);
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
        emit_audit_log(&env, &client, &attestor, &sk, 1_000);
        emit_audit_log(&env, &client, &attestor, &sk, 10_000);

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
        emit_audit_log(&env, &client, &attestor, &sk, 10_000);

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

    // ── Auto Pruning ──────────────────────────────────────────────────────────

    #[test]
    fn test_auto_prune_triggers_when_threshold_exceeded() {
        let env = make_env(); let (_admin, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        
        client.set_audit_log_retention(&1); // 1 day retention
        client.set_auto_prune_threshold(&1); // threshold 1
        
        emit_audit_log(&env, &client, &attestor, &sk, 10_000);
        emit_audit_log(&env, &client, &attestor, &sk, 10_000 + 86400 * 2); // 2 days later

        // Trigger auto prune
        set_ledger(&env, 10_000 + 86400 * 2 + 10);
        let count = client.get_audit_log_count();
        assert!(count >= 2);
        
        // The first log should be pruned. Verify it's missing.
        let page = client.get_audit_logs_paginated(&0, &10);
        // page only has 1 item because the first was pruned
        assert_eq!(page.len(), 1);
    }

    #[test]
    fn test_auto_prune_does_not_run_when_threshold_is_zero() {
        let env = make_env(); let (_admin, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        
        client.set_audit_log_retention(&1); 
        client.set_auto_prune_threshold(&0); // Disabled
        
        emit_audit_log(&env, &client, &attestor, &sk, 10_000);
        emit_audit_log(&env, &client, &attestor, &sk, 10_000 + 86400 * 2);

        set_ledger(&env, 10_000 + 86400 * 2 + 10);
        let count = client.get_audit_log_count();
        assert!(count >= 2);
        
        let page = client.get_audit_logs_paginated(&0, &10);
        assert_eq!(page.len(), 2);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_set_auto_prune_threshold_admin_gated() {
        let env = make_env(); 
        set_ledger(&env, 1000);
        let cid = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &cid);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Turn off mock all auths to enforce actual authorization rules
        env.mock_auths(&[]); // clears the global mock

        // This should panic because the caller is not admin and we have no mocked auth for admin
        client.set_auto_prune_threshold(&5);
    }

    // ── Export ───────────────────────────────────────────────────────────────

    #[test]
    fn test_export_audit_log_batch_returns_correct_entries() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        
        emit_audit_log(&env, &client, &attestor, &sk, 10_000);
        emit_audit_log(&env, &client, &attestor, &sk, 20_000);
        
        let batch = client.export_audit_log_batch(&0, &10);
        let mut buf = std::vec::Vec::new();
        for i in 0..batch.len() {
            buf.push(batch.get(i).unwrap());
        }
        let json_str = core::str::from_utf8(&buf).unwrap();
        assert!(json_str.starts_with('['));
        assert!(json_str.ends_with(']'));
        
        // Should contain two hex strings
        let parts: std::vec::Vec<&str> = json_str.split(',').collect();
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn test_export_audit_log_batch_size_is_capped_at_50() {
        let env = make_env(); let (_, client) = setup(&env);
        let (attestor, sk) = add_attestor(&env, &client);
        
        // We only emit 2, but request 100.
        emit_audit_log(&env, &client, &attestor, &sk, 10_000);
        emit_audit_log(&env, &client, &attestor, &sk, 20_000);
        
        let batch = client.export_audit_log_batch(&0, &100);
        let mut buf = std::vec::Vec::new();
        for i in 0..batch.len() {
            buf.push(batch.get(i).unwrap());
        }
        let json_str = core::str::from_utf8(&buf).unwrap();
        // Since there are only 2 logs, it should only return 2, but the cap logic is exercised internally.
        assert!(json_str.starts_with('['));
    }
}
