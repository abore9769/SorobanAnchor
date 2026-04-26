#!/bin/bash

set -e

echo "=== Running SorobanAnchor Performance Benchmarks ==="
echo ""

BASELINE_FILE="benchmark_baseline.json"
CURRENT_FILE="target/criterion/baseline.json"

# Run benchmarks
echo "Running benchmarks..."
cargo bench --bench load_benchmarks -- --save-baseline current

# Check if baseline exists
if [ -f "$BASELINE_FILE" ]; then
    echo ""
    echo "Comparing against baseline..."
    cargo bench --bench load_benchmarks -- --baseline current --load-baseline baseline
    
    # Simple performance regression check
    echo ""
    echo "Checking for performance regressions..."
    
    # This is a simplified check - in production, you'd parse Criterion's JSON output
    if cargo bench --bench load_benchmarks -- --baseline baseline 2>&1 | grep -q "Performance has regressed"; then
        echo "⚠️  WARNING: Performance regression detected!"
        echo "Review the benchmark results above for details."
        exit 1
    else
        echo "✅ No significant performance regressions detected"
    fi
else
    echo ""
    echo "No baseline found. Saving current results as baseline..."
    cp -r target/criterion "$BASELINE_FILE.dir" 2>/dev/null || true
    echo "Run this script again to compare future changes against this baseline."
fi

echo ""
echo "=== Benchmark Summary ==="
echo "Results saved to: target/criterion/"
echo "HTML reports available at: target/criterion/report/index.html"
echo ""
echo "Key metrics to monitor:"
echo "  - Single attestation throughput (ops/sec)"
echo "  - Batch registration latency (p50, p95, p99)"
echo "  - Rate limit check throughput under concurrency"
echo "  - Anchor routing performance with 50 candidates"
echo "  - Metadata cache lookup with 1000 entries"
echo ""
