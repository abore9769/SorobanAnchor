//! Tests for anchor health metric accumulation and reporting.
//!
//! Acceptance criteria verified:
//! 1. Health metrics start at zero for a new anchor.
//! 2. `record_health_event(success=true)` increments success_count.
//! 3. `record_health_event(success=false)` increments failure_count.
//! 4. `uptime_bps` is computed correctly after mixed events.
//! 5. `reset_anchor_health` zeroes all counters.
//! 6. Multiple anchors have independent metric stores.
//! 7. `last_event_at` reflects the ledger timestamp of the most recent event.
//! 8. Uptime is 10 000 bps (100 %) when all calls succeed.
//! 9. Uptime is 0 bps when all calls fail.

#![cfg(test)]

mod anchor_health_metrics_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Env,
    };

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

    // -----------------------------------------------------------------------
    // Initial state
    // -----------------------------------------------------------------------

    #[test]
    fn health_metrics_start_at_zero() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        let metrics = client.get_anchor_health(&anchor);
        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.failure_count, 0);
        assert_eq!(metrics.total_calls, 0);
        assert_eq!(metrics.uptime_bps, 0);
        assert_eq!(metrics.last_event_at, 0);
    }

    // -----------------------------------------------------------------------
    // Recording events
    // -----------------------------------------------------------------------

    #[test]
    fn record_success_increments_success_count() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        client.record_health_event(&anchor, &true);

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.success_count, 1);
        assert_eq!(m.failure_count, 0);
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.uptime_bps, 10_000); // 100 %
    }

    #[test]
    fn record_failure_increments_failure_count() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        client.record_health_event(&anchor, &false);

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.success_count, 0);
        assert_eq!(m.failure_count, 1);
        assert_eq!(m.total_calls, 1);
        assert_eq!(m.uptime_bps, 0); // 0 %
    }

    #[test]
    fn uptime_bps_computed_correctly_for_mixed_events() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        // 9 successes, 1 failure → 90 % = 9000 bps
        for _ in 0..9 {
            client.record_health_event(&anchor, &true);
        }
        client.record_health_event(&anchor, &false);

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.success_count, 9);
        assert_eq!(m.failure_count, 1);
        assert_eq!(m.total_calls, 10);
        assert_eq!(m.uptime_bps, 9_000);
    }

    #[test]
    fn uptime_bps_100_percent_all_success() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        for _ in 0..5 {
            client.record_health_event(&anchor, &true);
        }

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.uptime_bps, 10_000);
    }

    #[test]
    fn uptime_bps_0_percent_all_failure() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        for _ in 0..5 {
            client.record_health_event(&anchor, &false);
        }

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.uptime_bps, 0);
    }

    #[test]
    fn last_event_at_reflects_ledger_timestamp() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        client.record_health_event(&anchor, &true);
        assert_eq!(client.get_anchor_health(&anchor).last_event_at, 1000);

        set_ledger(&env, 5000);
        client.record_health_event(&anchor, &false);
        assert_eq!(client.get_anchor_health(&anchor).last_event_at, 5000);
    }

    #[test]
    fn counters_accumulate_across_multiple_calls() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        for _ in 0..3 {
            client.record_health_event(&anchor, &true);
        }
        for _ in 0..2 {
            client.record_health_event(&anchor, &false);
        }

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.success_count, 3);
        assert_eq!(m.failure_count, 2);
        assert_eq!(m.total_calls, 5);
        // 3/5 = 60 % = 6000 bps
        assert_eq!(m.uptime_bps, 6_000);
    }

    // -----------------------------------------------------------------------
    // Reset
    // -----------------------------------------------------------------------

    #[test]
    fn reset_anchor_health_zeroes_all_counters() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        for _ in 0..5 {
            client.record_health_event(&anchor, &true);
        }
        client.record_health_event(&anchor, &false);

        set_ledger(&env, 2000);
        client.reset_anchor_health(&anchor);

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.success_count, 0);
        assert_eq!(m.failure_count, 0);
        assert_eq!(m.total_calls, 0);
        assert_eq!(m.uptime_bps, 0);
        // last_event_at is set to the reset timestamp
        assert_eq!(m.last_event_at, 2000);
    }

    #[test]
    fn counters_accumulate_correctly_after_reset() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor = Address::generate(&env);

        for _ in 0..10 {
            client.record_health_event(&anchor, &false);
        }
        client.reset_anchor_health(&anchor);

        // Fresh start: 2 successes
        client.record_health_event(&anchor, &true);
        client.record_health_event(&anchor, &true);

        let m = client.get_anchor_health(&anchor);
        assert_eq!(m.success_count, 2);
        assert_eq!(m.failure_count, 0);
        assert_eq!(m.uptime_bps, 10_000);
    }

    // -----------------------------------------------------------------------
    // Independent anchors
    // -----------------------------------------------------------------------

    #[test]
    fn different_anchors_have_independent_health_stores() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _) = deploy(&env);
        let anchor_a = Address::generate(&env);
        let anchor_b = Address::generate(&env);

        for _ in 0..4 {
            client.record_health_event(&anchor_a, &true);
        }
        client.record_health_event(&anchor_b, &false);
        client.record_health_event(&anchor_b, &false);

        let ma = client.get_anchor_health(&anchor_a);
        let mb = client.get_anchor_health(&anchor_b);

        assert_eq!(ma.success_count, 4);
        assert_eq!(ma.failure_count, 0);
        assert_eq!(ma.uptime_bps, 10_000);

        assert_eq!(mb.success_count, 0);
        assert_eq!(mb.failure_count, 2);
        assert_eq!(mb.uptime_bps, 0);
    }
}
