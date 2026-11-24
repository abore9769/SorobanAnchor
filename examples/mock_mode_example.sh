#!/bin/bash
# Mock mode example - test without live anchors
#
# The mock-only feature exposes the `anchorkit::mock` module: pre-built
# response fixtures (SEP-6/24/38 responses, anchor capabilities) for Rust
# tests that must run without a live anchor. See the feature notes in
# Cargo.toml. The anchorkit CLI does not use these fixtures.
#
# Mode: Offline (after dependencies are fetched). The script compiles the
#   crate with the mock-only feature, then prints hard-coded sample output.
#
# Prerequisites:
#   Binary/build: Rust toolchain (cargo). Run from the repository root so
#                 cargo finds Cargo.toml
#   Environment:  None
#   Network:      None, except that the first cargo build may download crates
#
# Run:
#   bash examples/mock_mode_example.sh
#
# IMPORTANT: All output values below are SYNTHETIC strings hard-coded in this
# script. Transaction IDs, statuses, and webhook payloads shown here
# are NOT evidence of live anchor acceptance or real blockchain state.
#
# When to use mock mode:
# - Local development and testing
# - CI pipelines with no network access
# - Validating CLI command structure before connecting to live anchors
#
# When NOT to use mock mode:
# - Production deployments
# - Verifying actual transaction settlement
# - Testing live anchor integration behavior
#
# To enable mock mode, build with the mock-only feature flag:
#   cargo build --features mock-only

cargo build --features mock-only

echo "=== Mock Deposit Test ==="
# Sample deposit output (printed, not produced by the fixtures)
echo "Deposit initiated..."
echo "→ Transaction ID: mock-txn-dep-001 (SYNTHETIC)"

echo "=== Mock Withdrawal Test ==="
# Sample withdrawal output (printed, not produced by the fixtures)
echo "Withdrawal initiated..."
echo "→ Transaction ID: mock-txn-wdraw-001 (SYNTHETIC)"

echo "=== Mock Webhook Test ==="
# Sample webhook output (no webhook is sent)
echo "Webhook triggered: transaction.completed"
echo "→ Status: completed (SYNTHETIC)"

echo "Done!"
echo ""
echo "All transaction IDs and statuses shown above are synthetic mock values."
echo "They do not represent actual blockchain transactions or live anchor state."
