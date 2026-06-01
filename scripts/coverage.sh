#!/bin/bash
# Coverage metrics script for critical modules
# Generates test coverage reports for contract, rate_limiter, retry, and transaction_state_tracker

set -e

COVERAGE_DIR="coverage"
MODULES=("contract" "rate_limiter" "retry" "transaction_state_tracker")

echo "=== SorobanAnchor Test Coverage Report ==="
echo "Generating coverage metrics for critical modules..."
echo ""

# Create coverage directory
mkdir -p "$COVERAGE_DIR"

# Install tarpaulin if not present
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo "Installing cargo-tarpaulin..."
    cargo install cargo-tarpaulin
fi

# Generate coverage report
echo "Running coverage analysis..."
cargo tarpaulin \
    --out Html \
    --output-dir "$COVERAGE_DIR" \
    --exclude-files tests/* \
    --timeout 300 \
    --verbose

# Generate module-specific coverage summary
echo ""
echo "=== Module Coverage Summary ==="
for module in "${MODULES[@]}"; do
    echo ""
    echo "Module: $module"
    echo "File: src/${module}.rs"
    
    # Count lines and estimate coverage from test files
    if [ -f "src/${module}.rs" ]; then
        lines=$(wc -l < "src/${module}.rs")
        echo "  Total lines: $lines"
        
        # Check for corresponding test file
        test_file="tests/${module}_tests.rs"
        if [ -f "$test_file" ]; then
            test_lines=$(wc -l < "$test_file")
            echo "  Test file: $test_file"
            echo "  Test lines: $test_lines"
        else
            echo "  Test file: Not found (consider adding tests)"
        fi
    fi
done

echo ""
echo "=== Coverage Report Generated ==="
echo "HTML report: $COVERAGE_DIR/index.html"
echo ""
echo "Recommended coverage targets:"
echo "  - contract.rs: >= 85%"
echo "  - rate_limiter.rs: >= 90%"
echo "  - retry.rs: >= 90%"
echo "  - transaction_state_tracker.rs: >= 85%"
