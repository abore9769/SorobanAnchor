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
        Address, BytesN, Env,
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

    fn register_test_attestor(client: &AnchorKitContractClient, env: &Env, anchor: &Address, admin: &Address) {
        let session_id = client.create_session(admin);
        let pk = BytesN::from_array(env, &[0xABu8; 32]);
        client.register_attestor_with_session(admin, &session_id, anchor, &pk);
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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor, &admin);

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
        let (client, admin) = deploy(&env);
        let anchor_a = Address::generate(&env);
        let anchor_b = Address::generate(&env);
        register_test_attestor(&client, &env, &anchor_a, &admin);
        register_test_attestor(&client, &env, &anchor_b, &admin);

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

    // -----------------------------------------------------------------------
    // Maximum-counter overflow protection (issue: wrapped denominator)
    // -----------------------------------------------------------------------

    /// A window whose counters were restored from an external source can hold
    /// values near `u64::MAX`. Plain `success + failure` wraps `u64::MAX + 1`
    /// to `0`, which `success_rate()` then reads as an empty window and which
    /// collapses the denominator everywhere downstream.
    #[test]
    fn total_calls_maximum_counters_saturate_without_wrap() {
        let w = anchorkit::anchor_health::HealthWindow {
            started_at: 0,
            ended_at: 300,
            success_count: u64::MAX,
            failure_count: 1,
            p50_latency_ms: 0.0,
            routing_failure_count: 0,
            routing_attempt_count: 0,
            recovery_time_seconds: 0,
        };

        assert_eq!(w.total_calls(), u64::MAX, "u64::MAX + 1 must saturate, not wrap to 0");
        assert!((w.success_rate() - 1.0).abs() < 1e-12, "an all-success window must stay at 100%");
    }

    /// Both counters at maximum: the denominator must stay a single defined
    /// value rather than wrapping to `u64::MAX - 1` (or panicking under
    /// overflow checks).
    #[test]
    fn total_calls_both_counters_at_max_do_not_wrap() {
        let w = anchorkit::anchor_health::HealthWindow {
            started_at: 0,
            ended_at: 300,
            success_count: u64::MAX,
            failure_count: u64::MAX,
            p50_latency_ms: 0.0,
            routing_failure_count: 0,
            routing_attempt_count: 0,
            recovery_time_seconds: 0,
        };

        assert_eq!(w.total_calls(), u64::MAX);
        let rate = w.success_rate();
        assert!(rate.is_finite());
        assert!((0.0..=1.0).contains(&rate), "success_rate must stay in range, got {rate}");
    }

    /// Maximum counters restored through the checked constructor (i64 input)
    /// must still produce a usable denominator and rate.
    #[test]
    fn restored_maximum_counts_from_checked_constructor_are_usable() {
        let w = anchorkit::anchor_health::HealthWindow::new_checked(
            0, 300, i64::MAX, i64::MAX, 0.0, 0, 0, 0,
        )
        .unwrap();

        assert_eq!(w.success_count, i64::MAX as u64);
        assert_eq!(w.failure_count, i64::MAX as u64);
        assert_eq!(w.total_calls(), (i64::MAX as u64).saturating_add(i64::MAX as u64));
        let rate = w.success_rate();
        assert!(rate.is_finite());
        assert!((0.0..=1.0).contains(&rate));
    }

    /// `uptime_percent` shares the same wrap bug: `u64::MAX + 1` overflows to
    /// `0`, which the helper short-circuits to `0.0` and reports a completely
    /// failed anchor for what was an entirely successful run.
    #[test]
    fn uptime_percent_maximum_counters_do_not_wrap() {
        use anchorkit::anchor_health::uptime_percent;

        assert_eq!(uptime_percent(u64::MAX, 1), 100.0, "one success beyond u64::MAX must not report 0%");
        assert_eq!(uptime_percent(u64::MAX, 0), 100.0);
        assert_eq!(uptime_percent(0, u64::MAX), 0.0);
        assert!(uptime_percent(1, u64::MAX) > 0.0, "a single success must not be rounded away to exactly 0");

        let at_max = uptime_percent(u64::MAX, u64::MAX);
        assert!(at_max.is_finite());
        assert!((0.0..=100.0).contains(&at_max), "uptime must stay in range, got {at_max}");
    }

    /// Regression guard: ordinary inputs must return exactly what they did
    /// before the saturating fix.
    #[test]
    fn uptime_percent_ordinary_values_unchanged() {
        use anchorkit::anchor_health::uptime_percent;

        assert_eq!(uptime_percent(0, 0), 0.0);
        assert_eq!(uptime_percent(1, 0), 100.0);
        assert_eq!(uptime_percent(0, 1), 0.0);
        assert!((uptime_percent(9, 1) - 90.0).abs() < 1e-9);
        assert!((uptime_percent(1, 1) - 50.0).abs() < 1e-9);
        assert!((uptime_percent(3, 2) - 60.0).abs() < 1e-9);
        assert!((uptime_percent(999, 1) - 99.9).abs() < 1e-9);
        assert!((uptime_percent(12_345, 678) - (12_345f64 / 13_023f64) * 100.0).abs() < 1e-9);
    }

    /// Ordinary windows must keep their exact totals and rates.
    #[test]
    fn total_calls_ordinary_windows_unchanged() {
        let w = anchorkit::anchor_health::HealthWindow {
            started_at: 0,
            ended_at: 300,
            success_count: 9,
            failure_count: 1,
            p50_latency_ms: 100.0,
            routing_failure_count: 0,
            routing_attempt_count: 10,
            recovery_time_seconds: 0,
        };

        assert_eq!(w.total_calls(), 10);
        assert!((w.success_rate() - 0.9).abs() < 1e-12);

        let empty = anchorkit::anchor_health::HealthWindow {
            started_at: 0,
            ended_at: 300,
            success_count: 0,
            failure_count: 0,
            p50_latency_ms: 0.0,
            routing_failure_count: 0,
            routing_attempt_count: 0,
            recovery_time_seconds: 0,
        };
        assert_eq!(empty.total_calls(), 0);
        assert_eq!(empty.success_rate(), 0.0);
    }
}

// ---------------------------------------------------------------------------
// HealthWindow::new_checked — negative input rejection (issue #2)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod negative_count_rejection_tests {
    use anchorkit::anchor_health::HealthWindow;

    #[test]
    fn negative_success_count_is_rejected() {
        let err = HealthWindow::new_checked(0, 300, -1, 0, 0.0, 0, 0, 0).unwrap_err();
        assert!(
            err.message.contains("success_count"),
            "expected rejection of negative success_count, got: {}",
            err.message
        );
    }

    #[test]
    fn negative_failure_count_is_rejected() {
        let err = HealthWindow::new_checked(0, 300, 0, -5, 0.0, 0, 0, 0).unwrap_err();
        assert!(
            err.message.contains("failure_count"),
            "expected rejection of negative failure_count, got: {}",
            err.message
        );
    }

    #[test]
    fn negative_routing_failure_count_is_rejected() {
        let err = HealthWindow::new_checked(0, 300, 10, 0, 100.0, -2, 10, 0).unwrap_err();
        assert!(
            err.message.contains("routing_failure_count"),
            "expected rejection of negative routing_failure_count, got: {}",
            err.message
        );
    }

    #[test]
    fn negative_routing_attempt_count_is_rejected() {
        let err = HealthWindow::new_checked(0, 300, 10, 0, 100.0, 0, -1, 0).unwrap_err();
        assert!(
            err.message.contains("routing_attempt_count"),
            "expected rejection of negative routing_attempt_count, got: {}",
            err.message
        );
    }

    #[test]
    fn zero_counts_are_accepted() {
        let w = HealthWindow::new_checked(0, 300, 0, 0, 0.0, 0, 0, 0).unwrap();
        assert_eq!(w.success_count, 0);
        assert_eq!(w.failure_count, 0);
    }

    #[test]
    fn positive_counts_are_accepted_and_exact() {
        let w = HealthWindow::new_checked(100, 400, 95, 5, 200.0, 1, 10, 0).unwrap();
        assert_eq!(w.success_count, 95);
        assert_eq!(w.failure_count, 5);
        assert_eq!(w.routing_failure_count, 1);
        assert_eq!(w.routing_attempt_count, 10);
        // Aggregation should still classify this window as healthy
        use anchorkit::anchor_health::score_window;
        let score = score_window(&w);
        assert!(score.composite > 80.0, "healthy window should score >80, got {}", score.composite);
    }
}

// ---------------------------------------------------------------------------
// detect_health_transition — recovery transition observability (issue #3)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod recovery_transition_tests {
    use anchorkit::anchor_health::{detect_health_transition, HealthTransitionEvent};

    #[test]
    fn recovery_emits_exactly_one_signal() {
        // First cycle: unhealthy → healthy
        let ev = detect_health_transition(40.0, 85.0);
        match &ev {
            HealthTransitionEvent::Recovery { previous_composite, current_composite } => {
                assert!((previous_composite - 40.0).abs() < 1e-9);
                assert!((current_composite - 85.0).abs() < 1e-9);
            }
            _ => panic!("expected Recovery event, got {:?}", ev),
        }
    }

    #[test]
    fn repeated_healthy_observations_do_not_emit_duplicate_recovery() {
        // Both previous and current are healthy → NoChange, not a duplicate Recovery
        let ev = detect_health_transition(85.0, 92.0);
        assert_eq!(ev, HealthTransitionEvent::NoChange,
            "staying healthy must not produce a recovery event");
    }

    #[test]
    fn failure_transition_is_unchanged() {
        let ev = detect_health_transition(90.0, 30.0);
        assert!(ev.is_failure(), "healthy→non-healthy must emit Failure, got {:?}", ev);
    }

    #[test]
    fn no_change_when_both_unhealthy() {
        let ev = detect_health_transition(30.0, 45.0);
        assert_eq!(ev, HealthTransitionEvent::NoChange,
            "both non-healthy must emit NoChange");
    }

    #[test]
    fn boundary_at_exactly_80_is_healthy() {
        // Crossing the boundary from 79.9 → 80.0 is a recovery
        let ev = detect_health_transition(79.9, 80.0);
        assert!(ev.is_recovery(), "crossing to 80.0 must be a recovery, got {:?}", ev);
    }
}
