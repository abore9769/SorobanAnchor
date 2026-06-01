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
    use soroban_sdk::{Address, BytesN, Env};

    use crate::contract::{AnchorKitContract, AnchorKitContractClient, Attestation, Quote};
    use crate::errors::ErrorCode;

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
        BytesN::from_array(env, &[0xAB; 32])
    }

    fn dummy_payload_hash(env: &Env) -> soroban_sdk::Bytes {
        soroban_sdk::Bytes::from_array(env, &[0xCD; 32])
    }

    fn dummy_signature(env: &Env) -> soroban_sdk::Bytes {
        soroban_sdk::Bytes::from_array(env, &[0xEF; 64])
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
        let base_asset = soroban_sdk::String::from_small_str("USD");
        let quote_asset = soroban_sdk::String::from_small_str("EUR");

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
        let quote = client.get_quote(&quote_id);
        assert_eq!(quote.anchor, anchor);
        assert_eq!(quote.rate, 100u64);

        // Simulate upgrade
        client.upgrade(&dummy_wasm_hash(&env));

        // Verify quote is still accessible after upgrade
        let quote_after = client.get_quote(&quote_id);
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
        let session_id = client.create_session(&initiator, &3600u64);

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

        assert_eq!(client.get_schema_version(), 0);

        // Migrate to version 1
        client.migrate(&1u32);
        assert_eq!(client.get_schema_version(), 1);

        // Migrate to version 2
        client.migrate(&2u32);
        assert_eq!(client.get_schema_version(), 2);
    }

    /// Test that migration skipping versions is allowed
    #[test]
    fn migration_can_skip_versions() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        // Jump from 0 to 5
        client.migrate(&5u32);
        assert_eq!(client.get_schema_version(), 5);

        // Jump from 5 to 10
        client.migrate(&10u32);
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

        client.migrate(&1u32);
        // Attempting to migrate to the same version should fail
        client.migrate(&1u32);
    }

    /// Test that migration to lower version fails
    #[test]
    #[should_panic]
    fn migration_to_lower_version_fails() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        client.migrate(&5u32);
        // Attempting to downgrade should fail
        client.migrate(&3u32);
    }

    /// Test that migration to zero version fails
    #[test]
    #[should_panic]
    fn migration_to_zero_version_fails() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, admin) = deploy(&env);
        client.initialize(&admin);

        client.migrate(&1u32);
        // Attempting to migrate to zero should fail
        client.migrate(&0u32);
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
            &soroban_sdk::String::from_small_str("USD"),
            &soroban_sdk::String::from_small_str("EUR"),
            &100u64,
            &5u32,
            &1000u64,
            &10000u64,
            &2000u64,
        );

        let initiator = Address::generate(&env);
        let session_id = client.create_session(&initiator, &3600u64);

        // Upgrade contract
        client.upgrade(&dummy_wasm_hash(&env));

        // Verify all data is still accessible
        let attestation = client.get_attestation(&attestation_id);
        assert_eq!(attestation.issuer, issuer);

        let quote = client.get_quote(&quote_id);
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

        // Initial version should be 0
        assert_eq!(client.get_schema_version(), 0);

        // After migration to 1
        client.migrate(&1u32);
        assert_eq!(client.get_schema_version(), 1);

        // After migration to 3
        client.migrate(&3u32);
        assert_eq!(client.get_schema_version(), 3);

        // After migration to 10
        client.migrate(&10u32);
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
            &soroban_sdk::String::from_small_str("USD"),
            &soroban_sdk::String::from_small_str("EUR"),
            &100u64,
            &5u32,
            &1000u64,
            &10000u64,
            &2000u64,
        );

        let quote = client.get_quote(&quote_id);
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
                args: soroban_sdk::vec![&env, 1u32.into_val(&env)],
                sub_invokes: &[],
            },
        }]);
        client.migrate(&1u32);
    }
}
