//! Tests for on-chain cache invalidation governance (#555).

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env};
use anchorkit::cache_governance::{self, CacheGovernanceConfig};
use anchorkit::contract::AnchorKitContract;

fn make_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register_contract(None, AnchorKitContract);
    (env, cid)
}

fn set_ledger(env: &Env, seq: u32) {
    env.ledger().set(LedgerInfo {
        timestamp: 1000,
        protocol_version: 21,
        sequence_number: seq,
        network_id: Default::default(),
        base_reserve: 0,
        min_persistent_entry_ttl: 4096,
        min_temp_entry_ttl: 16,
        max_entry_ttl: 6_312_000,
    });
}

/// governance layer accepts proposals freely; contract layer enforces attestor check
#[test]
fn test_proposal_creation_and_retrieval() {
    let (env, cid) = make_env();
    set_ledger(&env, 1);
    let proposer = Address::generate(&env);
    let anchor = Address::generate(&env);
    env.as_contract(&cid, || {
        let pid = cache_governance::propose(&env, &proposer, &anchor);
        let proposal = cache_governance::get_proposal(&env, pid).unwrap();
        assert_eq!(proposal.proposer, proposer);
        assert_eq!(proposal.anchor, anchor);
        assert!(!proposal.executed);
        assert_eq!(proposal.endorsements.len(), 1); // proposer auto-endorses
    });
}

/// duplicate endorsement from the same address is silently ignored
#[test]
fn test_duplicate_endorsement_ignored() {
    let (env, cid) = make_env();
    set_ledger(&env, 1);
    let proposer = Address::generate(&env);
    let anchor = Address::generate(&env);

    env.as_contract(&cid, || {
        let pid = cache_governance::propose(&env, &proposer, &anchor);
        // proposer already endorsed on creation; endorsing again is a no-op
        let r = cache_governance::endorse(&env, &proposer, pid);
        assert!(r.is_ok());
        let proposal = cache_governance::get_proposal(&env, pid).unwrap();
        // still only 1 endorsement (the proposer)
        assert_eq!(proposal.endorsements.len(), 1);
    });
}

/// quorum met triggers invalidation exactly once
#[test]
fn test_quorum_met_triggers_invalidation() {
    let (env, cid) = make_env();
    set_ledger(&env, 1);
    let proposer = Address::generate(&env);
    let endorser1 = Address::generate(&env);
    let endorser2 = Address::generate(&env);
    let anchor = Address::generate(&env);

    env.as_contract(&cid, || {
        let cfg = CacheGovernanceConfig { quorum_threshold: 3, proposal_expiry_ledgers: 17_280 };
        cache_governance::set_config(&env, cfg);

        let pid = cache_governance::propose(&env, &proposer, &anchor); // 1 endorsement
        cache_governance::endorse(&env, &endorser1, pid).unwrap();     // 2
        cache_governance::endorse(&env, &endorser2, pid).unwrap();     // 3 — quorum

        let result = cache_governance::execute(&env, pid);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), anchor);

        // re-execute must fail
        let result2 = cache_governance::execute(&env, pid);
        assert!(result2.is_err());
    });
}

/// expired proposal cannot be executed
#[test]
fn test_expired_proposal_cannot_be_executed() {
    let (env, cid) = make_env();
    set_ledger(&env, 1);
    let proposer = Address::generate(&env);
    let endorser1 = Address::generate(&env);
    let endorser2 = Address::generate(&env);
    let anchor = Address::generate(&env);

    let pid = env.as_contract(&cid, || {
        let cfg = CacheGovernanceConfig { quorum_threshold: 3, proposal_expiry_ledgers: 10 };
        cache_governance::set_config(&env, cfg);
        let pid = cache_governance::propose(&env, &proposer, &anchor);
        cache_governance::endorse(&env, &endorser1, pid).unwrap();
        cache_governance::endorse(&env, &endorser2, pid).unwrap();
        pid
    });

    // advance past expiry
    set_ledger(&env, 11);

    env.as_contract(&cid, || {
        let result = cache_governance::execute(&env, pid);
        assert!(result.is_err(), "expired proposal must not execute");
    });
}

/// executed proposal cannot be re-executed
#[test]
fn test_executed_proposal_cannot_be_reexecuted() {
    let (env, cid) = make_env();
    set_ledger(&env, 1);
    let proposer = Address::generate(&env);
    let e1 = Address::generate(&env);
    let e2 = Address::generate(&env);
    let anchor = Address::generate(&env);

    env.as_contract(&cid, || {
        let cfg = CacheGovernanceConfig { quorum_threshold: 3, proposal_expiry_ledgers: 17_280 };
        cache_governance::set_config(&env, cfg);
        let pid = cache_governance::propose(&env, &proposer, &anchor);
        cache_governance::endorse(&env, &e1, pid).unwrap();
        cache_governance::endorse(&env, &e2, pid).unwrap();
        cache_governance::execute(&env, pid).unwrap();
        let r = cache_governance::execute(&env, pid);
        assert!(r.is_err(), "already-executed proposal must not execute again");
    });
}

/// admin can configure quorum and expiry
#[test]
fn test_admin_can_configure_quorum_and_expiry() {
    let (env, cid) = make_env();
    set_ledger(&env, 1);
    env.as_contract(&cid, || {
        let cfg = CacheGovernanceConfig { quorum_threshold: 5, proposal_expiry_ledgers: 500 };
        cache_governance::set_config(&env, cfg);
        let stored = cache_governance::get_config(&env);
        assert_eq!(stored.quorum_threshold, 5);
        assert_eq!(stored.proposal_expiry_ledgers, 500);
    });
}
