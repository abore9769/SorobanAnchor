use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _, LedgerInfo}, symbol_short, Address, BytesN, Env, Symbol, String};
use crate::domain_validator::validate_anchor_domain;
use crate::errors::{AnchorKitError, ErrorCode};
use crate::sep10_test_util::{build_sep10_jwt, register_attestor_with_sep10};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;

fn register_test_attestor(env: &Env, attestor: &Address) {
    let sk = SigningKey::generate(&mut OsRng);
    register_attestor_with_sep10(env, &{
        let contract_id = env.register_contract(None, AnchorKitContract);
        // We can't easily use the client here since we don't have it.
        // Use the contract directly via a client.
        AnchorKitContractClient::new(env, &contract_id)
    }, attestor, attestor, &sk);
}

#[test]
fn test_set_get_endpoint_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let attestor = Address::generate(&env);
    let endpoint = String::from_str(&env, "https://example.com/api");

    let sk = SigningKey::generate(&mut OsRng);
    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    register_attestor_with_sep10(&env, &client, &attestor, &attestor, &sk);

    client.set_endpoint(&attestor, &endpoint);
    let retrieved = client.get_endpoint(&attestor);
    assert_eq!(retrieved, endpoint);
}

#[test]
#[should_panic]
fn test_get_endpoint_not_registered() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let attestor = Address::generate(&env);
    client.get_endpoint(&attestor);
}

#[test]
#[should_panic]
fn test_set_endpoint_not_attestor() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let attestor = Address::generate(&env);
    let endpoint = String::from_str(&env, "https://example.com");
    client.set_endpoint(&attestor, &endpoint);
}

#[test]
#[should_panic]
fn test_set_endpoint_invalid_url() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let attestor = Address::generate(&env);
    let sk = SigningKey::generate(&mut OsRng);
    register_attestor_with_sep10(&env, &client, &attestor, &attestor, &sk);

    let invalid = String::from_str(&env, "http://invalid.com");
    client.set_endpoint(&attestor, &invalid);
}

#[test]
fn test_endpoint_updated_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let attestor = Address::generate(&env);
    let endpoint = String::from_str(&env, "https://test.com");

    let sk = SigningKey::generate(&mut OsRng);
    register_attestor_with_sep10(&env, &client, &attestor, &attestor, &sk);
    client.set_endpoint(&attestor, &endpoint);
}

