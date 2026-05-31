//! Tests for anchor endpoint proof-of-possession.
//!
//! Acceptance criteria verified:
//! 1. `register_endpoint_proof` stores a proof record on-chain.
//! 2. `verify_endpoint_proof` returns true when the hash matches.
//! 3. `verify_endpoint_proof` returns false on hash mismatch.
//! 4. `verify_endpoint_proof` returns false when no proof is registered.
//! 5. After successful verification the record is marked `verified = true`.
//! 6. `get_endpoint_proof` returns None before registration.
//! 7. `get_endpoint_proof` returns the stored record after registration.
//! 8. Off-chain `compute_pop_hash` / `verify_pop_challenge` helpers work correctly.
//! 9. Re-registering a proof overwrites the previous record.
//! 10. `register_endpoint_proof` rejects non-HTTPS endpoints.

#![cfg(test)]

mod proof_of_possession_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, BytesN, Env, String,
    };

    use anchorkit::anchor_health::{compute_pop_hash, verify_pop_challenge};
    use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient};

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
        client.initialize(&admin);
        (client, admin)
    }

    /// Register `anchor` as an attestor so `register_endpoint_proof` passes
    /// the `check_attestor` guard. We bypass the SEP-10 check by using a
    /// direct storage write via the contract's `register_attestor`-free path
    /// — in tests we mock all auths so we can call the admin method directly.
    fn register_attestor(client: &AnchorKitContractClient, env: &Env, anchor: &Address) {
        // Use the session-based registration which doesn't require a live SEP-10 token.
        // We create a session first, then register within it.
        let session_id = client.create_session(anchor, &3600u64);
        let pk = BytesN::from_array(env, &[0xABu8; 32]);
        client.register_attestor_with_session(anchor, &session_id, anchor, &pk);
    }

    fn make_proof_hash(env: &Env, challenge: &[u8; 32], endpoint: &str) -> BytesN<32> {
        let hash = compute_pop_hash(challenge, endpoint);
        BytesN::from_array(env, &hash)
    }

    // -----------------------------------------------------------------------
    // Off-chain helper unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn compute_pop_hash_is_deterministic() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let h1 = compute_pop_hash(challenge, endpoint);
        let h2 = compute_pop_hash(challenge, endpoint);
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_pop_hash_differs_on_different_endpoint() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let h1 = compute_pop_hash(challenge, "https://anchor.example.com");
        let h2 = compute_pop_hash(challenge, "https://other.example.com");
        assert_ne!(h1, h2);
    }

    #[test]
    fn compute_pop_hash_differs_on_different_challenge() {
        let endpoint = "https://anchor.example.com";
        let h1 = compute_pop_hash(b"challenge_a_bytes_32_chars______", endpoint);
        let h2 = compute_pop_hash(b"challenge_b_bytes_32_chars______", endpoint);
        assert_ne!(h1, h2);
    }

    #[test]
    fn verify_pop_challenge_succeeds_with_correct_inputs() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let hash = compute_pop_hash(challenge, endpoint);
        assert!(verify_pop_challenge(&hash, challenge, endpoint));
    }

    #[test]
    fn verify_pop_challenge_fails_wrong_challenge() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let hash = compute_pop_hash(challenge, endpoint);
        assert!(!verify_pop_challenge(&hash, b"wrong_challenge_bytes_32_chars__", endpoint));
    }

    #[test]
    fn verify_pop_challenge_fails_wrong_endpoint() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let hash = compute_pop_hash(challenge, endpoint);
        assert!(!verify_pop_challenge(&hash, challenge, "https://evil.example.com"));
    }

    // -----------------------------------------------------------------------
    // Contract-level proof-of-possession
    // -----------------------------------------------------------------------

    #[test]
    fn get_endpoint_proof_returns_none_before_registration() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        let result = client.get_endpoint_proof(&anchor);
        assert!(result.is_none());
    }

    #[test]
    fn register_and_retrieve_proof_record() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);
        register_attestor(&client, &env, &anchor);

        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint_str = "https://anchor.example.com";
        let endpoint = String::from_str(&env, endpoint_str);
        let proof_hash = make_proof_hash(&env, challenge, endpoint_str);

        client.register_endpoint_proof(&anchor, &endpoint, &proof_hash);

        let record = client.get_endpoint_proof(&anchor).unwrap();
        assert_eq!(record.anchor, anchor);
        assert_eq!(record.endpoint, endpoint);
        assert_eq!(record.proof_hash, proof_hash);
        assert_eq!(record.registered_at, 1000);
        assert!(!record.verified); // not yet verified
    }

    #[test]
    fn verify_endpoint_proof_returns_true_on_match() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);
        register_attestor(&client, &env, &anchor);

        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint_str = "https://anchor.example.com";
        let endpoint = String::from_str(&env, endpoint_str);
        let proof_hash = make_proof_hash(&env, challenge, endpoint_str);

        client.register_endpoint_proof(&anchor, &endpoint, &proof_hash);

        let result = client.verify_endpoint_proof(&anchor, &proof_hash);
        assert!(result);
    }

    #[test]
    fn verify_endpoint_proof_marks_record_as_verified() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);
        register_attestor(&client, &env, &anchor);

        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint_str = "https://anchor.example.com";
        let endpoint = String::from_str(&env, endpoint_str);
        let proof_hash = make_proof_hash(&env, challenge, endpoint_str);

        client.register_endpoint_proof(&anchor, &endpoint, &proof_hash);
        client.verify_endpoint_proof(&anchor, &proof_hash);

        let record = client.get_endpoint_proof(&anchor).unwrap();
        assert!(record.verified);
    }

    #[test]
    fn verify_endpoint_proof_returns_false_on_hash_mismatch() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);
        register_attestor(&client, &env, &anchor);

        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint_str = "https://anchor.example.com";
        let endpoint = String::from_str(&env, endpoint_str);
        let correct_hash = make_proof_hash(&env, challenge, endpoint_str);
        let wrong_hash = make_proof_hash(&env, b"wrong_challenge_bytes_32_chars__", endpoint_str);

        client.register_endpoint_proof(&anchor, &endpoint, &correct_hash);

        let result = client.verify_endpoint_proof(&anchor, &wrong_hash);
        assert!(!result);
    }

    #[test]
    fn verify_endpoint_proof_returns_false_when_no_proof_registered() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        let proof_hash = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.verify_endpoint_proof(&anchor, &proof_hash);
        assert!(!result);
    }

    #[test]
    fn re_registering_proof_overwrites_previous_record() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);
        register_attestor(&client, &env, &anchor);

        let endpoint_str = "https://anchor.example.com";
        let endpoint = String::from_str(&env, endpoint_str);

        let hash_v1 = make_proof_hash(&env, b"challenge_v1_bytes_32_chars_____", endpoint_str);
        client.register_endpoint_proof(&anchor, &endpoint, &hash_v1);
        // Verify v1 so it's marked verified
        client.verify_endpoint_proof(&anchor, &hash_v1);

        // Register a new proof (e.g. after challenge rotation)
        set_ledger(&env, 2000);
        let hash_v2 = make_proof_hash(&env, b"challenge_v2_bytes_32_chars_____", endpoint_str);
        client.register_endpoint_proof(&anchor, &endpoint, &hash_v2);

        let record = client.get_endpoint_proof(&anchor).unwrap();
        assert_eq!(record.proof_hash, hash_v2);
        assert_eq!(record.registered_at, 2000);
        // New registration resets verified flag
        assert!(!record.verified);

        // Old hash no longer verifies
        assert!(!client.verify_endpoint_proof(&anchor, &hash_v1));
        // New hash verifies
        assert!(client.verify_endpoint_proof(&anchor, &hash_v2));
    }

    #[test]
    fn register_endpoint_proof_rejects_non_https_endpoint() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);
        register_attestor(&client, &env, &anchor);

        let bad_endpoint = String::from_str(&env, "http://anchor.example.com");
        let proof_hash = BytesN::from_array(&env, &[0xABu8; 32]);

        let result = client.try_register_endpoint_proof(&anchor, &bad_endpoint, &proof_hash);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // End-to-end: off-chain compute → on-chain register → on-chain verify
    // -----------------------------------------------------------------------

    #[test]
    fn end_to_end_pop_flow() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);
        register_attestor(&client, &env, &anchor);

        // Step 1: anchor publishes challenge in stellar.toml (simulated here)
        let challenge: [u8; 32] = *b"prod_challenge_bytes_32_chars___";
        let endpoint_str = "https://anchor.example.com";

        // Step 2: off-chain monitor computes the proof hash
        let hash_bytes = compute_pop_hash(&challenge, endpoint_str);

        // Step 3: anchor registers the proof on-chain
        let endpoint = String::from_str(&env, endpoint_str);
        let proof_hash = BytesN::from_array(&env, &hash_bytes);
        client.register_endpoint_proof(&anchor, &endpoint, &proof_hash);

        // Step 4: verifier fetches the challenge from stellar.toml and verifies
        let stored = client.get_endpoint_proof(&anchor).unwrap();
        let stored_hash: [u8; 32] = stored.proof_hash.to_array();
        assert!(verify_pop_challenge(&stored_hash, &challenge, endpoint_str));

        // Step 5: verifier calls the contract to mark as verified
        assert!(client.verify_endpoint_proof(&anchor, &proof_hash));
        assert!(client.get_endpoint_proof(&anchor).unwrap().verified);
    }
}
