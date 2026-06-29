//! Migration tests for contract upgrade path and stored data compatibility.
//!
//! These tests verify that:
//! 1. Contract upgrades preserve stored data across versions
//! 2. Migration logic correctly handles schema changes
//! 3. Data accessibility is maintained after upgrades
//! 4. Invalid migration paths are rejected
//! 5. Rollback scenarios are handled safely

#![cfg(test)]

mod migration_tests {
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Bytes, BytesN, Env, IntoVal};

    use anchorkit::admin_audit_log::AdminAuditLog;
    use anchorkit::contract::{
        AnchorKitContract, AnchorKitContractClient, QuoteV1, SCHEMA_V1, SCHEMA_V2,
    };
    use anchorkit::deterministic_hash::make_storage_key;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

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

    fn deploy(env: &Env) -> (AnchorKitContractClient, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        (client, admin)
    }

    fn dummy_wasm_hash(env: &Env) -> BytesN<32> {
        let wasm: &[u8] = &[
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x16, 0x11,
            0x63, 0x6f, 0x6e, 0x74, 0x72, 0x61, 0x63, 0x74,
            0x65, 0x6e, 0x76, 0x6d, 0x65, 0x74, 0x61, 0x76, 0x30,
            0x00, 0x00, 0x00, 0x00,
        ];
        env.deployer().upload_contract_wasm(Bytes::from_slice(env, wasm))
    }

    fn dummy_payload_hash(env: &Env) -> soroban_sdk::Bytes {
        soroban_sdk::Bytes::from_array(env, &[0xCD; 32])
    }

    fn dummy_signature(env: &Env) -> soroban_sdk::Bytes {
        soroban_sdk::Bytes::from_array(env, &[0xEF; 64])
    }

    fn write_legacy_quote_v1(env: &Env, anchor: &Address, quote: QuoteV1) {
        let xdr = anchor.to_xdr(env);
        let anchor_raw: alloc::vec::Vec<u8> =
            (0..xdr.len()).map(|i| xdr.get(i).unwrap()).collect();
        let q_key = make_storage_key(env, &[b"QUOTE", &anchor_raw, &quote.quote_id.to_be_bytes()]);
        env.storage().persistent().set(&q_key, &quote);

        let idx_key = soroban_sdk::Symbol::new(env, "QUOTE_INDEX");
        let mut ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&idx_key)
            .unwrap_or_else(|| soroban_sdk::Vec::new(env));
        ids.push_back(quote.quote_id);
        env.storage().persistent().set(&idx_key, &ids);

        let ref_key = make_storage_key(env, &[b"QANCH", &quote.quote_id.to_be_bytes()]);
        env.storage().persistent().set(&ref_key, anchor);
    }

    fn cursor_exists(env: &Env) -> bool {
        let cursor_key = soroban_sdk::Symbol::new(env, "MIGRATE_QUOTES_V2_CURSOR");
        env.storage().persistent().has(&cursor_key)
    }

    // -----------------------------------------------------------------------
    // Data Preservation Tests
    // -----------------------------------------------------------------------

    /// Test that attestations are preserved across contract upgrades
    #[test]
    fn attestations_preserved_after_upgrade() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        // Submit an attestation before upgrade
        let issuer = Address::generate(&env);
        let subject = Address::generate(&env);
        let payload_hash = dummy_payload_hash(&env);
        let signature = dummy_signature(&env);

        let attestation_id = client.submit_attestation(
            &issuer,
            &subject,
            &1000u64,
            &payload_hash,
            &signature,
        );

        // Verify attestation exists
        let attestation = client.get_attestation(&attestation_id);
        assert_eq!(attestation.issuer, issuer);
        assert_eq!(attestation.subject, subject);

        // Simulate upgrade (in test environment, this is a no-op)
        client.upgrade(&dummy_wasm_hash(&env));

        // Verify attestation is still accessible after upgrade
        let attestation_after = client.get_attestation(&attestation_id);
        assert_eq!(attestation_after.issuer, issuer);
        assert_eq!(attestation_after.subject, subject);
        assert_eq!(attestation_after.timestamp, 1000u64);
    }

    /// Test that quotes are preserved across contract upgrades
    #[test]
    fn quotes_preserved_after_upgrade() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let anchor = Address::generate(&env);
        let base_asset = soroban_sdk::String::from_str(&env, "USD");
        let quote_asset = soroban_sdk::String::from_str(&env, "USD");

        // Submit a quote before upgrade
        let quote_id = client.submit_quote(
            &anchor,
            &base_asset,
            &quote_asset,
            &100u64,
            &5u32,
            &1000u64,
            &10000u64,
            &2000u64,
        );

        // Verify quote exists
        let quote = client.get_quote(&anchor, &quote_id);
        assert_eq!(quote.anchor, anchor);
        assert_eq!(quote.rate, 100u64);

        // Simulate upgrade
        client.upgrade(&dummy_wasm_hash(&env));

        // Verify quote is still accessible after upgrade
        let quote_after = client.get_quote(&anchor, &quote_id);
        assert_eq!(quote_after.anchor, anchor);
        assert_eq!(quote_after.rate, 100u64);
        assert_eq!(quote_after.fee_percentage, 5u32);
    }

    /// Test that sessions are preserved across contract upgrades
    #[test]
    fn sessions_preserved_after_upgrade() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let initiator = Address::generate(&env);

        // Create a session before upgrade
        let session_id = client.create_session(&initiator);

        // Verify session exists
        let session = client.get_session(&session_id);
        assert_eq!(session.initiator, initiator);
        assert!(!session.closed);

        // Simulate upgrade
        client.upgrade(&dummy_wasm_hash(&env));

        // Verify session is still accessible after upgrade
        let session_after = client.get_session(&session_id);
        assert_eq!(session_after.initiator, initiator);
        assert!(!session_after.closed);
    }

    // -----------------------------------------------------------------------
    // Migration Path Tests
    // -----------------------------------------------------------------------

    /// Test that migration to a higher version succeeds
    #[test]
    fn migration_to_higher_version_succeeds() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        assert_eq!(client.get_schema_version(), 1);

        // Migrate to version 2
        client.migrate(&2u32, &100u32);
        assert_eq!(client.get_schema_version(), 2);

        // Migrate to version 3
        client.migrate(&3u32, &100u32);
        assert_eq!(client.get_schema_version(), 3);
    }

    /// Test that migration skipping versions is allowed
    #[test]
    fn migration_can_skip_versions() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        // Jump from 1 to 5
        client.migrate(&5u32, &100u32);
        assert_eq!(client.get_schema_version(), 5);

        // Jump from 5 to 10
        client.migrate(&10u32, &100u32);
        assert_eq!(client.get_schema_version(), 10);
    }

    /// Test that migration to same version fails
    #[test]
    #[should_panic]
    fn migration_to_same_version_fails() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        client.migrate(&2u32, &100u32);
        // Attempting to migrate to the same version should fail
        client.migrate(&2u32, &100u32);
    }

    /// Test that migration to lower version fails
    #[test]
    #[should_panic]
    fn migration_to_lower_version_fails() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        client.migrate(&5u32, &100u32);
        // Attempting to downgrade should fail
        client.migrate(&3u32, &100u32);
    }

    /// Test that migration to zero version fails
    #[test]
    #[should_panic]
    fn migration_to_zero_version_fails() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        client.migrate(&2u32, &100u32);
        // Attempting to migrate to zero should fail
        client.migrate(&0u32, &100u32);
    }

    // -----------------------------------------------------------------------
    // Data Compatibility Tests
    // -----------------------------------------------------------------------

    /// Test that multiple data types are preserved together
    #[test]
    fn multiple_data_types_preserved_after_upgrade() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        // Create multiple data items
        let issuer = Address::generate(&env);
        let subject = Address::generate(&env);
        let attestation_id = client.submit_attestation(
            &issuer,
            &subject,
            &1000u64,
            &dummy_payload_hash(&env),
            &dummy_signature(&env),
        );

        let anchor = Address::generate(&env);
        let quote_id = client.submit_quote(
            &anchor,
            &soroban_sdk::String::from_str(&env, "USD"),
            &soroban_sdk::String::from_str(&env, "USD"),
            &100u64,
            &5u32,
            &1000u64,
            &10000u64,
            &2000u64,
        );

        let initiator = Address::generate(&env);
        let session_id = client.create_session(&initiator);

        // Upgrade contract
        client.upgrade(&dummy_wasm_hash(&env));

        // Verify all data is still accessible
        let attestation = client.get_attestation(&attestation_id);
        assert_eq!(attestation.issuer, issuer);

        let quote = client.get_quote(&anchor, &quote_id);
        assert_eq!(quote.anchor, anchor);

        let session = client.get_session(&session_id);
        assert_eq!(session.initiator, initiator);
    }

    /// Test that data remains consistent across multiple upgrades
    #[test]
    fn data_consistent_across_multiple_upgrades() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let issuer = Address::generate(&env);
        let subject = Address::generate(&env);
        let attestation_id = client.submit_attestation(
            &issuer,
            &subject,
            &1000u64,
            &dummy_payload_hash(&env),
            &dummy_signature(&env),
        );

        // First upgrade
        client.upgrade(&dummy_wasm_hash(&env));
        let attestation_v1 = client.get_attestation(&attestation_id);

        // Second upgrade
        client.upgrade(&dummy_wasm_hash(&env));
        let attestation_v2 = client.get_attestation(&attestation_id);

        // Data should be identical
        assert_eq!(attestation_v1.issuer, attestation_v2.issuer);
        assert_eq!(attestation_v1.subject, attestation_v2.subject);
        assert_eq!(attestation_v1.timestamp, attestation_v2.timestamp);
    }

    // -----------------------------------------------------------------------
    // Schema Version Tests
    // -----------------------------------------------------------------------

    /// Test that schema version is tracked correctly
    #[test]
    fn schema_version_tracked_correctly() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        // Initial version should be 1 after initialize
        assert_eq!(client.get_schema_version(), 1);

        // After migration to 2
        client.migrate(&2u32, &100u32);
        assert_eq!(client.get_schema_version(), 2);

        // After migration to 3
        client.migrate(&3u32, &100u32);
        assert_eq!(client.get_schema_version(), 3);

        // After migration to 10
        client.migrate(&10u32, &100u32);
        assert_eq!(client.get_schema_version(), 10);
    }

    /// Test that attestations include schema version
    #[test]
    fn attestations_include_schema_version() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let issuer = Address::generate(&env);
        let subject = Address::generate(&env);
        let attestation_id = client.submit_attestation(
            &issuer,
            &subject,
            &1000u64,
            &dummy_payload_hash(&env),
            &dummy_signature(&env),
        );

        let attestation = client.get_attestation(&attestation_id);
        // Schema version should be set (typically 1 for current version)
        assert!(attestation.schema_version > 0);
    }

    /// Test that quotes include schema version
    #[test]
    fn quotes_include_schema_version() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let anchor = Address::generate(&env);
        let quote_id = client.submit_quote(
            &anchor,
            &soroban_sdk::String::from_str(&env, "USD"),
            &soroban_sdk::String::from_str(&env, "USD"),
            &100u64,
            &5u32,
            &1000u64,
            &10000u64,
            &2000u64,
        );

        let quote = client.get_quote(&anchor, &quote_id);
        // Schema version should be set
        assert!(quote.schema_version > 0);
    }

    // -----------------------------------------------------------------------
    // Upgrade Authorization Tests
    // -----------------------------------------------------------------------

    /// Test that only admin can perform upgrades
    #[test]
    #[should_panic]
    fn non_admin_cannot_upgrade() {
        let env = Env::default();
        set_ledger(&env, 1000);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: soroban_sdk::vec![&env, admin.clone().into_val(&env)],
                sub_invokes: &[],
            },
        }]);
        client.initialize(&admin);

        let attacker = Address::generate(&env);
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &attacker,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "upgrade",
                args: soroban_sdk::vec![&env, dummy_wasm_hash(&env).into_val(&env)],
                sub_invokes: &[],
            },
        }]);
        client.upgrade(&dummy_wasm_hash(&env));
    }

    /// Test that only admin can perform migrations
    #[test]
    #[should_panic]
    fn non_admin_cannot_migrate() {
        let env = Env::default();
        set_ledger(&env, 1000);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: soroban_sdk::vec![&env, admin.clone().into_val(&env)],
                sub_invokes: &[],
            },
        }]);
        client.initialize(&admin);

        let attacker = Address::generate(&env);
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &attacker,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "migrate",
                args: soroban_sdk::vec![&env, 2u32.into_val(&env), 100u32.into_val(&env)],
                sub_invokes: &[],
            },
        }]);
        client.migrate(&2u32, &100u32);
    }

    // -----------------------------------------------------------------------
    // Quote schema v2 migration tests (#559)
    // -----------------------------------------------------------------------

    #[test]
    fn v1_quote_readable_after_migration() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let anchor = Address::generate(&env);
        let base_asset = soroban_sdk::String::from_str(&env, "USD");
        let quote_asset = soroban_sdk::String::from_str(&env, "EUR");

        write_legacy_quote_v1(
            &env,
            &anchor,
            QuoteV1 {
                quote_id: 1,
                anchor: anchor.clone(),
                base_asset: base_asset.clone(),
                quote_asset: quote_asset.clone(),
                rate: 150,
                fee_percentage: 25,
                minimum_amount: 100,
                maximum_amount: 10_000,
                valid_until: 2000,
                schema_version: SCHEMA_V1,
            },
        );

        client.migrate(&2u32, &100u32);

        let quote = client.get_quote(&anchor, &1u64);
        assert_eq!(quote.schema_version, SCHEMA_V2);
        assert_eq!(quote.routing_reason, None);
        assert_eq!(quote.rate, 150);
        assert_eq!(quote.fee_percentage, 25);
        assert!(!cursor_exists(&env));
    }

    #[test]
    fn quote_migration_is_idempotent() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let anchor = Address::generate(&env);
        client.submit_quote(
            &anchor,
            &soroban_sdk::String::from_str(&env, "USD"),
            &soroban_sdk::String::from_str(&env, "USD"),
            &100u64,
            &5u32,
            &1000u64,
            &10000u64,
            &2000u64,
        );

        client.migrate(&2u32, &100u32);
        let after_first = client.get_quote(&anchor, &1u64);
        assert_eq!(after_first.schema_version, SCHEMA_V2);

        let audit_count_after_first = AdminAuditLog::get_entry_count(&env);
        client.migrate(&3u32, &100u32);
        let after_second = client.get_quote(&anchor, &1u64);
        assert_eq!(after_second.schema_version, SCHEMA_V2);
        assert_eq!(after_second.rate, after_first.rate);
        assert_eq!(AdminAuditLog::get_entry_count(&env), audit_count_after_first);
    }

    #[test]
    fn quote_migration_advances_cursor_in_batches() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let anchor = Address::generate(&env);
        let usd = soroban_sdk::String::from_str(&env, "USD");
        for _ in 0..3 {
            client.submit_quote(&anchor, &usd, &usd, &100u64, &5u32, &1000u64, &10000u64, &2000u64);
        }

        assert_eq!(client.get_schema_version(), SCHEMA_V1);
        client.migrate(&2u32, &1u32);
        assert!(cursor_exists(&env));
        assert_eq!(client.get_schema_version(), SCHEMA_V1);

        client.migrate(&2u32, &1u32);
        assert!(cursor_exists(&env));

        client.migrate(&2u32, &1u32);
        assert!(!cursor_exists(&env));
        assert_eq!(client.get_schema_version(), SCHEMA_V2);

        let quote = client.get_quote(&anchor, &3u64);
        assert_eq!(quote.schema_version, SCHEMA_V2);
    }

    #[test]
    fn quote_migration_logs_admin_audit_per_batch() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        let anchor = Address::generate(&env);
        let usd = soroban_sdk::String::from_str(&env, "USD");
        client.submit_quote(&anchor, &usd, &usd, &100u64, &5u32, &1000u64, &10000u64, &2000u64);
        client.submit_quote(&anchor, &usd, &usd, &110u64, &5u32, &1000u64, &10000u64, &2000u64);

        let before = AdminAuditLog::get_entry_count(&env);
        client.migrate(&2u32, &1u32);
        let entry0 = AdminAuditLog::get_entry(&env, before).expect("batch audit entry");
        assert_eq!(entry0.change_type, soroban_sdk::String::from_str(&env, "schema_migration"));
        assert_eq!(entry0.target, soroban_sdk::String::from_str(&env, "quotes"));
        assert_eq!(entry0.old_value, soroban_sdk::String::from_str(&env, "v1"));
        assert_eq!(entry0.new_value, soroban_sdk::String::from_str(&env, "v2 (1)"));

        client.migrate(&2u32, &1u32);
        let entry1 = AdminAuditLog::get_entry(&env, before + 1).expect("second batch audit entry");
        assert_eq!(entry1.new_value, soroban_sdk::String::from_str(&env, "v2 (1)"));
    }

    #[test]
    #[should_panic]
    fn non_admin_cannot_migrate_quotes_to_v2() {
        let env = Env::default();
        set_ledger(&env, 1000);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: soroban_sdk::vec![&env, admin.clone().into_val(&env)],
                sub_invokes: &[],
            },
        }]);
        client.initialize(&admin);

        let anchor = Address::generate(&env);
        write_legacy_quote_v1(
            &env,
            &anchor,
            QuoteV1 {
                quote_id: 1,
                anchor: anchor.clone(),
                base_asset: soroban_sdk::String::from_str(&env, "USD"),
                quote_asset: soroban_sdk::String::from_str(&env, "USD"),
                rate: 100,
                fee_percentage: 5,
                minimum_amount: 1000,
                maximum_amount: 10000,
                valid_until: 2000,
                schema_version: SCHEMA_V1,
            },
        );

        let attacker = Address::generate(&env);
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &attacker,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "migrate",
                args: soroban_sdk::vec![&env, 2u32.into_val(&env), 100u32.into_val(&env)],
                sub_invokes: &[],
            },
        }]);
        client.migrate(&2u32, &100u32);
    }
}
