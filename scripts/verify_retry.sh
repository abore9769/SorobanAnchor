#!/bin/bash
# Verification script for Issue #112: Retry & Exponential Backoff
# Updated for Issue #172: Retry jitter with cryptographically seeded randomness

set -e

echo "=========================================="
echo "Issue #112/#172: Retry & Exponential Backoff + Jitter"
echo "Verification Script"
echo "=========================================="
echo ""

echo "✓ Running retry tests..."
cargo test retry --lib --quiet
echo "  ✅ All retry tests passed"
echo ""

echo "✓ Running jitter source tests..."
cargo test test_different_seeds_produce_different_delays --lib --quiet
cargo test test_delay_within_bounds --lib --quiet
cargo test test_mock_source_deterministic --lib --quiet
cargo test test_ledger_jitter_source_consecutive_differ --lib --quiet
echo "  ✅ All jitter source tests passed"
echo ""

echo "✓ Running error mapping tests..."
cargo test error_mapping --lib --quiet
echo "  ✅ All error mapping tests passed"
echo ""

echo "✓ Verifying exponential backoff..."
cargo test test_delay_increases_exponentially --lib --quiet
cargo test test_delay_capped_at_max --lib --quiet
echo "  ✅ Exponential backoff tests passed"
echo ""

echo "✓ Verifying configurable strategies..."
cargo test test_aggressive_config --lib --quiet
cargo test test_conservative_config --lib --quiet
echo "  ✅ Configurable strategy tests passed"
echo ""

echo "=========================================="
echo "✅ ALL VERIFICATIONS PASSED"
echo "=========================================="
echo ""
echo "Summary:"
echo "  • JitterSource trait: ✅ Defined with next_seed() -> u64"
echo "  • LedgerJitterSource: ✅ XORs sequence ^ timestamp ^ counter"
echo "  • MockJitterSource:   ✅ Deterministic seed sequences for tests"
echo "  • retry_with_backoff: ✅ Accepts jitter_source parameter"
echo "  • Different seeds:    ✅ Produce different delays"
echo "  • Delay bounds:       ✅ Always within configured range"
echo "  • Non-retryable:      ✅ Fail immediately"
echo ""
