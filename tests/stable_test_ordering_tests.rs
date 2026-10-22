//! Regression test for stable test ordering (issue #1315).
//!
//! Tests must not rely on shared global state or the order in which other
//! tests have run.  This file provides a focused repeatability check for the
//! `sort_quotes` helper, which is exercised extensively in
//! `tests/deterministic_ordering_tests.rs`.
//!
//! The strategy:
//! - Each test builds its own fixtures from scratch (no shared `lazy_static`
//!   or `once_cell`; no `static mut`; no global counter).
//! - The sort result is asserted both in isolation and alongside a different
//!   sort call in the same function body, confirming order-independence.
//! - Two tests call the same sort function with the same inputs in different
//!   textual orders to verify neither depends on the other having run first.
//!
//! Acceptance criteria (from issue #1315):
//! - The selected test passes independently (no prior test required).
//! - Its result does not depend on another test having run first.
//! - The scope is confined to one test module.

#![cfg(test)]
#![cfg(not(feature = "wasm"))]

mod stable_ordering_isolation_tests {
    use anchorkit::sep38::{sort_quotes, FirmQuote, QuoteSortOrder};

    // -----------------------------------------------------------------------
    // Isolated fixture builder — called fresh in every test body.
    // No shared state: no statics, no lazy_static, no global counters.
    // -----------------------------------------------------------------------

    fn make_quote(id: &str, expires_at: u64, price: &str) -> FirmQuote {
        FirmQuote {
            id: id.into(),
            expires_at,
            price: price.into(),
            sell_amount: "500".into(),
            buy_amount: "75".into(),
            sell_asset: "XLM".into(),
            buy_asset: "USDC".into(),
            routing_reason: None,
        }
    }

    // -----------------------------------------------------------------------
    // Test A — passes when run alone.
    //
    // Uses only locally-constructed fixtures; the result must not change
    // whether this test runs first, last, or in isolation via:
    //   cargo test stable_ordering_isolation_tests::price_asc_is_stable_when_run_alone
    // -----------------------------------------------------------------------

    #[test]
    fn price_asc_is_stable_when_run_alone() {
        // All fixtures created in this scope — no shared state.
        let low  = make_quote("low",  5000, "0.10");
        let mid  = make_quote("mid",  5000, "0.20");
        let high = make_quote("high", 5000, "0.30");

        let result = sort_quotes(&[high.clone(), low.clone(), mid.clone()], QuoteSortOrder::PriceAsc);

        assert_eq!(result.len(), 3, "all three quotes must be present in the output");
        assert_eq!(result[0].price, "0.10", "cheapest quote must be first");
        assert_eq!(result[1].price, "0.20", "middle quote must be second");
        assert_eq!(result[2].price, "0.30", "most expensive quote must be last");
    }

    // -----------------------------------------------------------------------
    // Test B — same assertion, different input permutation.
    //
    // If `price_asc_is_stable_when_run_alone` left any state behind (e.g. a
    // sorted cache or a global counter), this test might return a different
    // result when run after it versus when run first.  The identical assertion
    // on independently-built data catches that class of bug.
    // -----------------------------------------------------------------------

    #[test]
    fn price_asc_same_result_regardless_of_prior_test() {
        // Freshly constructed fixtures — same logical data as Test A but
        // assembled independently in this stack frame.
        let low  = make_quote("low",  5000, "0.10");
        let mid  = make_quote("mid",  5000, "0.20");
        let high = make_quote("high", 5000, "0.30");

        // Feed them in a different input order to this call.
        let result = sort_quotes(&[mid.clone(), high.clone(), low.clone()], QuoteSortOrder::PriceAsc);

        // Expect the identical output ordering as Test A, confirming the
        // function is stateless and the result does not depend on Test A
        // having run (or not run) before this test.
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].price, "0.10");
        assert_eq!(result[1].price, "0.20");
        assert_eq!(result[2].price, "0.30");
    }

    // -----------------------------------------------------------------------
    // Test C — two consecutive calls in the same test body produce identical
    // output, confirming no internal mutable state accumulates between calls.
    // -----------------------------------------------------------------------

    #[test]
    fn repeated_calls_in_same_test_produce_identical_output() {
        let a = make_quote("alpha", 3000, "0.15");
        let b = make_quote("beta",  1000, "0.15");
        let c = make_quote("gamma", 2000, "0.15");

        // All prices are equal, so the tie-break by id (ascending) must be
        // applied consistently on every call.
        let first_call  = sort_quotes(&[a.clone(), b.clone(), c.clone()], QuoteSortOrder::PriceAsc);
        let second_call = sort_quotes(&[a.clone(), b.clone(), c.clone()], QuoteSortOrder::PriceAsc);

        let ids_first:  Vec<&str> = first_call.iter().map(|q| q.id.as_str()).collect();
        let ids_second: Vec<&str> = second_call.iter().map(|q| q.id.as_str()).collect();

        assert_eq!(
            ids_first, ids_second,
            "two consecutive calls with the same input must produce the same output; \
             a mismatch would indicate accumulated mutable state inside sort_quotes"
        );

        // Also verify the expected tie-break order (id ascending).
        assert_eq!(ids_first, vec!["alpha", "beta", "gamma"]);
    }
}
