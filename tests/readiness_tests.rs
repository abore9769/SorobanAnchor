#![cfg(test)]

#[path = "sep10_test_util.rs"]
mod sep10_test_util;

mod readiness_tests {
    use soroban_sdk::{
        testutils::{Address as _, LedgerInfo},
        Address, Env, Vec,
    };

    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    use anchorkit::contract::{
        AnchorKitContract, AnchorKitContractClient, SERVICE_DEPOSITS, SERVICE_QUOTES,
        SERVICE_WITHDRAWALS,
    };
    use crate::sep10_test_util::register_attestor_with_sep10;

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1_000,
            protocol_version: 21,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });
        env
    }

    fn setup(env: &Env) -> (AnchorKitContractClient, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    fn services_vec(env: &Env, vals: &[u32]) -> Vec<u32> {
        let mut v = Vec::new(env);
        for &s in vals {
            v.push_back(s);
        }
        v
    }

    // -----------------------------------------------------------------------
    // Unregistered anchor: all readiness flags are false
    // -----------------------------------------------------------------------

    #[test]
    fn test_readiness_unregistered_anchor() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);

        let report = client.get_anchor_readiness(&anchor);

        assert!(!report.is_registered);
        assert!(!report.deposit_ready);
        assert!(!report.withdrawal_ready);
        assert!(!report.quote_ready);
        assert!(!report.kyc_ready);
    }

    // -----------------------------------------------------------------------
    // Registered anchor with no services: is_registered=true, rest false
    // -----------------------------------------------------------------------

    #[test]
    fn test_readiness_registered_no_services() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);

        let report = client.get_anchor_readiness(&anchor);

        assert!(report.is_registered);
        assert!(!report.deposit_ready);
        assert!(!report.withdrawal_ready);
        assert!(!report.quote_ready);
        assert!(!report.kyc_ready);
    }

    // -----------------------------------------------------------------------
    // Anchor with deposit service: deposit_ready=true
    // -----------------------------------------------------------------------

    #[test]
    fn test_readiness_deposit_service_configured() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);
        client.configure_services(&anchor, &services_vec(&env, &[SERVICE_DEPOSITS]));

        let report = client.get_anchor_readiness(&anchor);

        assert!(report.deposit_ready);
        assert!(!report.withdrawal_ready);
        assert!(!report.quote_ready);
    }

    // -----------------------------------------------------------------------
    // is_deposit_ready helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_deposit_ready_true_after_configure() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);
        client.configure_services(&anchor, &services_vec(&env, &[SERVICE_DEPOSITS]));

        assert!(client.is_deposit_ready(&anchor));
    }

    #[test]
    fn test_is_deposit_ready_false_no_services() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);

        assert!(!client.is_deposit_ready(&anchor));
    }

    // -----------------------------------------------------------------------
    // is_quote_ready: false when no quote configured
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_quote_ready_false_no_quote() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);
        client.configure_services(
            &anchor,
            &services_vec(&env, &[SERVICE_DEPOSITS, SERVICE_QUOTES]),
        );

        assert!(!client.is_quote_ready(&anchor));
    }

    // -----------------------------------------------------------------------
    // is_quote_ready: true when valid quote exists
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_quote_ready_true_with_valid_quote() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);
        client.configure_services(
            &anchor,
            &services_vec(&env, &[SERVICE_DEPOSITS, SERVICE_QUOTES]),
        );

        let base = soroban_sdk::String::from_str(&env, "USDC");
        let quote_asset = soroban_sdk::String::from_str(&env, "XLM");
        // valid_until = current timestamp + 3600
        client.submit_quote(&anchor, &base, &quote_asset, &100u64, &100u32, &1u64, &10_000u64, &4600u64);

        assert!(client.is_quote_ready(&anchor));
    }

    // -----------------------------------------------------------------------
    // Multiple services: all configured flags appear in report
    // -----------------------------------------------------------------------

    #[test]
    fn test_readiness_all_services() {
        let env = make_env();
        let (client, _) = setup(&env);
        let anchor = Address::generate(&env);
        let sk = SigningKey::generate(&mut OsRng);
        register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);
        client.configure_services(
            &anchor,
            &services_vec(&env, &[SERVICE_DEPOSITS, SERVICE_WITHDRAWALS]),
        );

        let report = client.get_anchor_readiness(&anchor);

        assert!(report.is_registered);
        assert!(report.deposit_ready);
        assert!(report.withdrawal_ready);
    }
}
