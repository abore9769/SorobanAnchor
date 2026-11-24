#!/usr/bin/env bash
# offline_mode_example.sh — Demonstrate offline config validation and workflow
# simulation without any network access.
#
# Mode: Offline. Runs `anchorkit offline validate` / `offline simulate` via
#   `cargo run`; these read local files only and need no credentials.
#
# Prerequisites:
#   Binary/build: Rust toolchain (cargo, native target). The script builds and
#                 runs the CLI with `cargo run --bin anchorkit`, so no
#                 installed binary is needed. Run from the repository root:
#                 it reads configs/ by relative path
#   Environment:  None
#   Network:      None, except that the first cargo build may download crates
#
# Run:
#   bash examples/offline_mode_example.sh

set -euo pipefail

CLI="cargo run --bin anchorkit --"

echo "=== AnchorKit Offline Mode Examples ==="
echo

# ── 1. Validate all configs in configs/ ──────────────────────────────────────
echo "1. Validate all configs (offline):"
$CLI offline validate
echo

# ── 2. Validate a specific config file ───────────────────────────────────────
echo "2. Validate a single config file:"
$CLI offline validate --config configs/fiat-on-off-ramp.json
echo

# ── 3. Simulate a deploy workflow ────────────────────────────────────────────
echo "3. Simulate a deploy workflow:"
$CLI offline simulate --workflow deploy
echo

# ── 4. Simulate a register workflow ──────────────────────────────────────────
echo "4. Simulate a register workflow:"
$CLI offline simulate --workflow register
echo

# ── 5. Simulate an attest workflow ───────────────────────────────────────────
echo "5. Simulate an attest workflow:"
$CLI offline simulate --workflow attest
echo

# ── 6. CI / pre-deployment gate (non-zero exit on failure) ───────────────────
echo "6. Pre-deployment gate (fails if any config is invalid):"
if $CLI offline validate; then
    echo "   All configs valid — safe to proceed with deployment."
else
    echo "   Config validation FAILED — deployment blocked." >&2
    exit 1
fi
