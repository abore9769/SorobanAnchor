#![cfg(test)]

mod proof_of_possession_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Bytes, Env, String,
    };

    use crate::contract::{AnchorKitContract, AnchorKitContractClient};

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

    /// Register an admin + attestor and store a 32-byte dummy verifying key.
    fn setup_attestor(env: &Env) -> (Address, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);

        let admin = Address::generate(env);
        let attestor = Address::generate(env);

        client.initialize(&admin);

        // Register a dummy SEP-10 verifying key (32 zero bytes).
        let pk = Bytes::from_array(env, &[0u8; 32]);
        client.set_sep10_jwt_verifying_key(&attestor, &pk);

        // Register the attestor (mock auth bypasses JWT check).
        client.register_attestor(
            &attestor,
            &String::from_str(env, "mock_token"),
            &Address::generate(env),
        );

        (contract_id, attestor)
    }

    // -----------------------------------------------------------------------
    // issue_pop_challenge
    // -----------------------------------------------------------------------

    #[test]
    fn test_issue_pop_challenge_returns_nonce() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let challenge = client.issue_pop_challenge(&attestor, &300u64);

        assert_eq!(challenge.attestor, attestor);
        assert_eq!(challenge.nonce.len(), 16);
        assert!(!challenge.verified);
        // expires_at = now + ttl
        assert_eq!(challenge.expires_at, 1_000_000 + 300);
    }

    #[test]
    fn test_issue_pop_challenge_ttl_capped_at_3600() {
        let env = make_env();
        set_ledger(&env, 2_000_000);
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        // Request a TTL larger than the 3600-second cap.
        let challenge = client.issue_pop_challenge(&attestor, &99999u64);
        assert_eq!(challenge.expires_at, 2_000_000 + 3600);
    }

    #[test]
    fn test_issue_pop_challenge_zero_ttl_uses_default() {
        let env = make_env();
        set_ledger(&env, 500_000);
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        let challenge = client.issue_pop_challenge(&attestor, &0u64);
        assert_eq!(challenge.expires_at, 500_000 + 3600);
    }

    #[test]
    #[should_panic]
    fn test_issue_pop_challenge_unregistered_attestor_panics() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Attestor was never registered.
        let stranger = Address::generate(&env);
        client.issue_pop_challenge(&stranger, &300u64);
    }

    // -----------------------------------------------------------------------
    // get_pop_challenge
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_pop_challenge_returns_stored_challenge() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.issue_pop_challenge(&attestor, &600u64);
        let fetched = client.get_pop_challenge(&attestor);

        assert_eq!(fetched.attestor, attestor);
        assert_eq!(fetched.nonce.len(), 16);
        assert!(!fetched.verified);
    }

    #[test]
    #[should_panic]
    fn test_get_pop_challenge_no_challenge_panics() {
        let env = make_env();
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        // No challenge issued yet.
        client.get_pop_challenge(&attestor);
    }

    // -----------------------------------------------------------------------
    // verify_pop_response — failure paths (success path requires real Ed25519)
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic]
    fn test_verify_pop_response_wrong_sig_length_panics() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.issue_pop_challenge(&attestor, &300u64);

        // Signature is only 32 bytes — must be 64.
        let bad_sig = Bytes::from_array(&env, &[0u8; 32]);
        client.verify_pop_response(&attestor, &bad_sig);
    }

    #[test]
    #[should_panic]
    fn test_verify_pop_response_expired_challenge_panics() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        // Issue a 60-second challenge.
        client.issue_pop_challenge(&attestor, &60u64);

        // Advance ledger past expiry.
        set_ledger(&env, 1_000_000 + 61);

        let sig = Bytes::from_array(&env, &[0u8; 64]);
        client.verify_pop_response(&attestor, &sig);
    }

    #[test]
    #[should_panic]
    fn test_verify_pop_response_no_challenge_panics() {
        let env = make_env();
        set_ledger(&env, 1_000_000);
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        // No challenge issued.
        let sig = Bytes::from_array(&env, &[0u8; 64]);
        client.verify_pop_response(&attestor, &sig);
    }

    // -----------------------------------------------------------------------
    // get_pop_status — no result yet
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic]
    fn test_get_pop_status_before_verification_panics() {
        let env = make_env();
        let (contract_id, attestor) = setup_attestor(&env);
        let client = AnchorKitContractClient::new(&env, &contract_id);

        client.get_pop_status(&attestor);
    }
}
