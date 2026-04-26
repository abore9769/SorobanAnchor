#!/bin/bash

echo "=== Testing AnchorKit Doctor Command ==="
echo ""

echo "Test 1: Running doctor without environment variables"
echo "Expected: Some checks should fail or warn"
echo "---"
unset ANCHOR_CONTRACT_ID
unset ANCHOR_ADMIN_SECRET
cargo run --bin anchorkit -- doctor
RESULT1=$?
echo ""

echo "Test 2: Running doctor with partial environment"
echo "Expected: Some warnings but should complete"
echo "---"
export ANCHOR_ADMIN_SECRET="SBADMINSECRETKEY123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
cargo run --bin anchorkit -- doctor
RESULT2=$?
echo ""

echo "Test 3: Running doctor with full environment"
echo "Expected: Most checks should pass"
echo "---"
export ANCHOR_CONTRACT_ID="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"
export ANCHOR_ADMIN_SECRET="SBADMINSECRETKEY123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
cargo run --bin anchorkit -- doctor --network testnet
RESULT3=$?
echo ""

echo "Test 4: Running doctor with --fix flag"
echo "Expected: Should attempt to fix issues"
echo "---"
cargo run --bin anchorkit -- doctor --fix
RESULT4=$?
echo ""

echo "=== Test Summary ==="
echo "Test 1 exit code: $RESULT1 (expected: non-zero)"
echo "Test 2 exit code: $RESULT2"
echo "Test 3 exit code: $RESULT3"
echo "Test 4 exit code: $RESULT4"

if [ $RESULT1 -ne 0 ]; then
    echo "✅ Doctor command tests completed!"
    echo "Note: Exit codes may vary based on your environment setup"
    exit 0
else
    echo "⚠ Test 1 should have failed without environment variables"
    exit 1
fi
