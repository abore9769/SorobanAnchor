# Test Coverage Metrics

This document describes the test coverage strategy and targets for SorobanAnchor's critical modules.

## Overview

Production readiness requires visibility into which code paths are exercised by tests. SorobanAnchor maintains coverage targets for the most critical modules that handle core functionality.

## Coverage Targets

| Module | Target | Rationale |
|--------|--------|-----------|
| `contract.rs` | >= 85% | Core contract logic, admin functions, attestations |
| `rate_limiter.rs` | >= 90% | Security-critical rate limiting enforcement |
| `retry.rs` | >= 90% | Reliability-critical retry and backoff logic |
| `transaction_state_tracker.rs` | >= 85% | State management and audit trail |

## Generating Coverage Reports

### Prerequisites

Install `cargo-tarpaulin`:

```bash
cargo install cargo-tarpaulin
```

### Generate Coverage

Run the coverage script:

```bash
./scripts/coverage.sh
```

This generates:
- HTML coverage report in `coverage/index.html`
- Module-specific coverage summaries
- Recommendations for improving coverage

### Manual Coverage Analysis

To generate coverage for specific modules:

```bash
cargo tarpaulin --out Html --output-dir coverage --exclude-files tests/*
```

## Coverage by Module

### contract.rs

**Critical paths to cover:**
- Contract initialization with admin setup
- Attestor registration and revocation
- Attestation submission with replay protection
- Session creation and management
- Quote submission and retrieval
- Routing logic with fee/reputation scoring
- Audit log recording
- Configuration changes

**Test files:**
- `tests/cli_integration_harness.rs` - End-to-end workflows
- `tests/admin_permission_tests.rs` - Admin operations
- `tests/attestation_sig_tests.rs` - Attestation logic
- `tests/session_tests.rs` - Session management
- `tests/routing_tests.rs` - Routing logic

### rate_limiter.rs

**Critical paths to cover:**
- Rate limit window calculation
- Submission count tracking
- Throttling enforcement
- Window expiration and reset
- Health check reporting

**Test files:**
- `tests/load_simulation_tests.rs` - High-concurrency scenarios
- `tests/health_check_tests.rs` - Health reporting

### retry.rs

**Critical paths to cover:**
- Exponential backoff calculation
- Retry attempt counting
- Timeout enforcement
- Error classification for retry eligibility
- Max retry limit enforcement

**Test files:**
- `tests/cross_platform_tests.rs` - Retry behavior across platforms
- `tests/load_simulation_tests.rs` - Retry under load

### transaction_state_tracker.rs

**Critical paths to cover:**
- State transition validation
- Audit trail recording
- Recovery logic
- State persistence
- Timestamp tracking

**Test files:**
- `tests/transaction_state_tracker_tests.rs` - State transitions
- `tests/ledger_boundary_tests.rs` - Boundary conditions
- `tests/cli_integration_harness.rs` - End-to-end state tracking

## Improving Coverage

### Adding Tests

When coverage falls below targets:

1. Identify uncovered lines in the HTML report
2. Determine if the code path is critical or defensive
3. Add tests to exercise the path
4. Re-run coverage to verify improvement

### Coverage Gaps

Common reasons for coverage gaps:

- **Error paths**: Add tests that trigger error conditions
- **Edge cases**: Test boundary values and state transitions
- **Defensive code**: Verify panic conditions are tested
- **Feature gates**: Ensure all feature combinations are tested

## CI Integration

Coverage reports are generated in CI on every PR. Coverage regressions are flagged for review.

### Coverage Thresholds

- **Hard failure**: Coverage drops below 80% for critical modules
- **Warning**: Coverage drops below target for any critical module
- **Info**: Coverage report available in artifacts

## Maintenance

Coverage targets should be reviewed quarterly:

1. Assess if targets are realistic and achievable
2. Identify modules that consistently exceed targets
3. Identify modules that struggle to meet targets
4. Adjust targets based on risk assessment

## References

- [Tarpaulin Documentation](https://github.com/xd009642/tarpaulin)
- [Coverage Best Practices](https://en.wikipedia.org/wiki/Code_coverage)
