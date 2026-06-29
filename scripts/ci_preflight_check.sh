#!/bin/bash
# Repository sanity check script for SorobanAnchor.
# Verifies the core Rust contract and library layout are intact.

set -e

echo "SorobanAnchor repository sanity check"
echo ""

FAILURES=0

check_pass() { echo -e "${GREEN}✓${NC} $1"; }
check_fail() { echo -e "${RED}✗${NC} $1"; FAILURES=$((FAILURES + 1)); }
check_warn() { echo -e "${YELLOW}⚠${NC} $1"; }

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "1. FILE STRUCTURE CHECKS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

for f in Cargo.toml src/lib.rs src/contract.rs src/errors.rs README.md Makefile; do
    if [ -f "$f" ]; then
        check_pass "$f exists"
    else
        check_fail "$f missing"
    fi
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "2. FEATURE FLAG CONSISTENCY"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if grep -q '#\[cfg\(feature = "std"\)\]' src/main.rs; then
    check_pass "main.rs gated behind std feature"
else
    check_fail "main.rs NOT gated behind std feature"
fi

if grep -q '#\[cfg\(feature = "std"\)\]' src/config.rs; then
    check_pass "config.rs gated behind std feature"
else
    check_fail "config.rs NOT gated behind std feature"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "3. MODULE EXPORTS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

for m in contract errors sep10_jwt rate_limiter retry replay_detection transaction_state_tracker domain_validator deterministic_hash anchor_health service_management admin_audit_log; do
    if grep -q "mod $m;" src/lib.rs || grep -q "pub mod $m;" src/lib.rs; then
        check_pass "Module $m declared in lib.rs"
    else
        check_fail "Module $m NOT declared in lib.rs"
    fi
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "4. NO_UNWRAP IN PRODUCTION CODE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

NON_TEST_UNWRAPS=$(grep -rn "\.unwrap()" src/ --include="*.rs" | grep -v "fn test_" | grep -v "#\[cfg(test)\]" | wc -l || echo "0")
if [ "$NON_TEST_UNWRAPS" -eq 0 ]; then
    check_pass "No unwrap() in production source code"
else
    check_warn "Found $NON_TEST_UNWRAPS unwrap() calls in production source code"
fi

echo ""
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "5. REPRODUCIBLE BUILD VERIFICATION"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if bash "$SCRIPT_DIR/verify_reproducible_build.sh"; then
    check_pass "Reproducible WASM build verified"
else
    check_fail "Reproducible WASM build check failed"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "6. SUMMARY"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}✓ ALL CHECKS PASSED${NC}"
    exit 0
else
    echo -e "${RED}✗ $FAILURES CHECK(S) FAILED${NC}"
    exit 1
fi
