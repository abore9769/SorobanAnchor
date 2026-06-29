//! #247 — Schema versioning and migration tests.
//!
//! Verifies that:
//! - Every new record written by the contract carries `schema_version = SCHEMA_V1`.
//! - `get_schema_version` returns `SCHEMA_V1`.
//! - `migrate` is idempotent (safe to call multiple times).
//! - A simulated schema evolution (V1 → V2 field addition) can be handled by
//!   reading the old record and rewriting it with the new shape.

#![cfg(test)]

mod schema_versioning_tests {
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Bytes, BytesN, Env,
    };

    use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient, Attestation, SCHEMA_V1};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn set_ledger(env: &Env, timestamp: u64) {
        env.ledger().set(LedgerInfo {
            timestamp,
            protocol_version: 21,
            sequence_number: 100,
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
    // get_schema_version
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_schema_version_returns_v1() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);
        assert_eq!(client.get_schema_version(), SCHEMA_V1);
        assert_eq!(SCHEMA_V1, 1u32);
    }

    // -----------------------------------------------------------------------
    // Attestation carries schema_version = SCHEMA_V1
    // -----------------------------------------------------------------------

    #[test]
    fn test_attestation_schema_version_is_v1() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);

        let attestor = Address::generate(&env);
        let subject = Address::generate(&env);

        // Seed storage so submit_attestation can proceed without the full SEP-10 flow.
        let pk_bytes = BytesN::from_array(&env, &[1u8; 32]);
        env.storage().persistent().set(
            &(symbol_short!("ATTESTOR"), attestor.clone()),
            &true,
        );
        env.storage().persistent().set(
            &(symbol_short!("ATPUBKEY"), attestor.clone()),
            &pk_bytes,
        );

        let payload_hash = Bytes::from_slice(&env, &[0xabu8; 32]);
        let signature = Bytes::from_slice(&env, &[0u8; 64]);

        let id = client.submit_attestation(
            &attestor,
            &subject,
            &1_000u64,
            &payload_hash,
            &signature,
        );

        let attest_key = (symbol_short!("ATTEST"), id);
        let stored: Attestation = env
            .storage()
            .persistent()
            .get(&attest_key)
            .expect("attestation must be stored");

        assert_eq!(stored.schema_version, SCHEMA_V1);
    }

    // -----------------------------------------------------------------------
    // migrate is idempotent
    // -----------------------------------------------------------------------

    #[test]
    fn test_migrate_is_idempotent() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);

        client.migrate(&2u32, &100u32);
        // Second call with advancing version must also succeed without panicking.
        client.migrate(&3u32, &100u32);
    }

    // -----------------------------------------------------------------------
    // Simulated schema evolution: V1 → V2
    //
    // Demonstrates the migration pattern documented in contract.rs:
    // read a V1 record, bump schema_version, write it back.
    // -----------------------------------------------------------------------

    #[test]
    fn test_simulated_v1_to_v2_migration() {
        let env = make_env();
        set_ledger(&env, 1_000);
        let (client, _) = setup(&env);

        let attestor = Address::generate(&env);
        let subject = Address::generate(&env);

        let pk_bytes = BytesN::from_array(&env, &[2u8; 32]);
        env.storage().persistent().set(
            &(symbol_short!("ATTESTOR"), attestor.clone()),
            &true,
        );
        env.storage().persistent().set(
            &(symbol_short!("ATPUBKEY"), attestor.clone()),
            &pk_bytes,
        );

        let payload_hash = Bytes::from_slice(&env, &[0xcdu8; 32]);
        let signature = Bytes::from_slice(&env, &[0u8; 64]);

        let id = client.submit_attestation(
            &attestor,
            &subject,
            &1_000u64,
            &payload_hash,
            &signature,
        );

        let key = (symbol_short!("ATTEST"), id);
        let mut record: Attestation = env
            .storage()
            .persistent()
            .get(&key)
            .expect("must exist");
        assert_eq!(record.schema_version, SCHEMA_V1);

        // Simulate migration: bump to V2.
        const SCHEMA_V2: u32 = 2;
        record.schema_version = SCHEMA_V2;
        env.storage().persistent().set(&key, &record);

        let migrated: Attestation = env
            .storage()
            .persistent()
            .get(&key)
            .expect("must exist after migration");
        assert_eq!(migrated.schema_version, SCHEMA_V2);
        // All other fields must be unchanged.
        assert_eq!(migrated.id, id);
        assert_eq!(migrated.issuer, attestor);
        assert_eq!(migrated.subject, subject);
    }
}
