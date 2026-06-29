#![cfg(test)]

mod sep10_test_util;

mod batch_attestation_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Bytes, Env, Vec,
    };
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient, AttestationInput, MAX_BATCH_SIZE};
    use anchorkit::errors::ErrorCode;
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

    fn setup(env: &Env) -> (AnchorKitContractClient, Address, SigningKey) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);

        // Set rate limit high enough for batch tests (max 100 per window)
        client.set_rate_limit_config(&100u32, &1000u32);

        let sk = SigningKey::generate(&mut OsRng);
        let attestor = Address::generate(env);
        register_attestor_with_sep10(env, &client, &attestor, &attestor, &sk);
        (client, attestor, sk)
    }

    fn make_payload(env: &Env, seed: u8) -> Bytes {
        let mut b = Bytes::new(env);
        for i in 0..32u8 {
            b.push_back(seed.wrapping_add(i));
        }
        b
    }

    fn make_input(
        env: &Env,
        issuer: &Address,
        sk: &SigningKey,
        seed: u8,
    ) -> AttestationInput {
        let subject = Address::generate(env);
        let payload_hash = make_payload(env, seed);
        let signature = sign_payload(env, sk, &payload_hash);
        AttestationInput {
            issuer: issuer.clone(),
            subject,
            timestamp: 1_000_001u64,
            payload_hash,
            signature,
        }
    }

    // ── successful batch ───────────────────────────────────────────────────

    #[test]
    fn test_successful_batch_of_five() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);

        let inputs: Vec<AttestationInput> = {
            let mut v = Vec::new(&env);
            for seed in 0u8..5 {
                v.push_back(make_input(&env, &attestor, &sk, seed));
            }
            v
        };

        let ids = client.submit_attestation_batch(&attestor, &inputs);
        assert_eq!(ids.len(), 5);
        // IDs must be sequential (0..4 if this is the first batch)
        for i in 0..5u32 {
            assert_eq!(ids.get(i).unwrap(), i as u64);
        }
    }

    // ── empty batch ────────────────────────────────────────────────────────

    #[test]
    fn test_empty_batch_returns_empty_vec() {
        let env = make_env();
        let (client, attestor, _sk) = setup(&env);

        let inputs: Vec<AttestationInput> = Vec::new(&env);
        let ids = client.submit_attestation_batch(&attestor, &inputs);
        assert_eq!(ids.len(), 0);
    }

    // ── duplicate rejected, zero state changes ────────────────────────────

    #[test]
    fn test_batch_with_duplicate_is_fully_rejected() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);

        // Submit a single attestation first to seed a replay entry
        let ph = make_payload(&env, 0xAA);
        let sig = sign_payload(&env, &sk, &ph);
        let first_id = client.submit_attestation(&attestor, &Address::generate(&env), &1_000_001u64, &ph, &sig);

        // Build a batch that includes the same payload_hash (duplicate)
        let inputs: Vec<AttestationInput> = {
            let mut v = Vec::new(&env);
            // Fresh entry first
            v.push_back(make_input(&env, &attestor, &sk, 0x01));
            // Duplicate entry
            let dup_sig = sign_payload(&env, &sk, &ph);
            v.push_back(AttestationInput {
                issuer: attestor.clone(),
                subject: Address::generate(&env),
                timestamp: 1_000_001u64,
                payload_hash: ph.clone(),
                signature: dup_sig,
            });
            v
        };

        let result = client.try_submit_attestation_batch(&attestor, &inputs);
        assert!(result.is_err(), "batch with duplicate must be rejected");

        // The fresh entry (seed 0x01) must NOT have been committed: next id
        // if it had been would be first_id+1, but retrieval of first_id+1 must fail.
        let next_fetch = client.try_get_attestation(&(first_id + 1));
        assert!(next_fetch.is_err(), "the fresh batch entry must not have been committed");
    }

    // ── invalid signature rejected ─────────────────────────────────────────

    #[test]
    fn test_batch_with_invalid_signature_is_fully_rejected() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);

        let wrong_sk = SigningKey::generate(&mut OsRng);

        let inputs: Vec<AttestationInput> = {
            let mut v = Vec::new(&env);
            v.push_back(make_input(&env, &attestor, &sk, 0x10));
            // Bad sig
            let ph = make_payload(&env, 0x11);
            let bad_sig = sign_payload(&env, &wrong_sk, &ph);
            v.push_back(AttestationInput {
                issuer: attestor.clone(),
                subject: Address::generate(&env),
                timestamp: 1_000_001u64,
                payload_hash: ph,
                signature: bad_sig,
            });
            v
        };

        let result = client.try_submit_attestation_batch(&attestor, &inputs);
        assert!(result.is_err(), "batch with invalid signature must be rejected");
        // ID 0 must not exist
        assert!(client.try_get_attestation(&0u64).is_err(), "no attestation must have been committed");
    }

    // ── batch exceeding MAX_BATCH_SIZE ─────────────────────────────────────

    #[test]
    fn test_batch_exceeding_max_size_is_rejected() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);

        let inputs: Vec<AttestationInput> = {
            let mut v = Vec::new(&env);
            for seed in 0u8..=(MAX_BATCH_SIZE as u8) {
                v.push_back(make_input(&env, &attestor, &sk, seed));
            }
            v
        };

        let result = client.try_submit_attestation_batch(&attestor, &inputs);
        assert!(result.is_err(), "batch exceeding MAX_BATCH_SIZE must be rejected");
    }

    // ── proportional rate-limit enforcement ───────────────────────────────

    #[test]
    fn test_proportional_rate_limit_enforcement() {
        let env = make_env();
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Set a tight rate limit: max 8 slots per window
        // BATCH_ATTESTATION_RATE_MULTIPLIER = 5, batch of 2 → 10 slots → exceeds 8
        client.set_rate_limit_config(&8u32, &1000u32);

        let sk = SigningKey::generate(&mut OsRng);
        let attestor = Address::generate(&env);
        register_attestor_with_sep10(&env, &client, &attestor, &attestor, &sk);

        // batch of 2 requires 2*5 = 10 slots but limit is 8 → rejected
        let inputs: Vec<AttestationInput> = {
            let mut v = Vec::new(&env);
            v.push_back(make_input(&env, &attestor, &sk, 0x20));
            v.push_back(make_input(&env, &attestor, &sk, 0x21));
            v
        };

        let result = client.try_submit_attestation_batch(&attestor, &inputs);
        assert!(result.is_err(), "batch of 2 consuming 10 rate slots must be rejected when limit is 8");
    }
}
