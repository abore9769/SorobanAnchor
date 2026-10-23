# AnchorKit Testing Guide

This guide covers the comprehensive testing infrastructure for AnchorKit, including property-based testing, fuzzing, and feature-flag matrix testing.

## Overview

AnchorKit uses three layers of testing to ensure robustness:

1. **Property-Based Testing** — Automated generation of test inputs to uncover edge cases
2. **Fuzzing** — Continuous random input generation to find crashes and security issues
3. **Feature-Flag Matrix Testing** — Validation across all supported build configurations

## Running One Integration Test

Cargo's `--test` option selects an integration-test target, while the optional
test-name argument filters tests within that target:

```bash
# Run one test by name within the ledger boundary integration target
cargo test --test ledger_boundary_tests test_rate_limit_window_expires_exactly_at_boundary

# Run every test in that integration target
cargo test --test ledger_boundary_tests
```

Cargo matches the supplied test name as a substring. These commands run the
local integration tests; they do not enable the separately gated live-test
workflow.

## Property-Based Testing (#643)

Property-based testing generates randomized valid inputs and verifies the system behaves correctly under all conditions.

### Running Property-Based Tests

```bash
# Run all property-based tests
cargo test --test property_based_parsing_tests

# Run specific property test
cargo test --test property_based_parsing_tests prop_sep6_deposit_response_with_valid_fields

# Run with verbose output
cargo test --test property_based_parsing_tests -- --nocapture

# Run with multiple samples (default is 256)
PROPTEST_CASES=1000 cargo test --test property_based_parsing_tests
```

### Test Coverage

The property-based test suite covers:

- **SEP-6 Deposit Responses** — Transaction IDs, statuses, addresses, amounts
- **SEP-6 Withdrawal Responses** — Minimal and full field combinations
- **SEP-24 Transactions** — Memos, transaction states
- **SEP-38 Quotes** — Price representations, amounts
- **Domain Validation** — Arbitrary domains, case sensitivity
- **Response Schema Versioning** — Version resolution and ordering
- **JSON Payload Robustness** — Extra fields, idempotency, numeric bounds
- **Error Responses** — Simple and nested error structures

### Example Property Test

```rust
#[test]
fn prop_sep6_deposit_response_with_valid_fields() {
    proptest!(|(
        txn_id in valid_transaction_id(),
        status in valid_status(),
        address in valid_address(),
        how in valid_json_string(),
    )| {
        let response = json!({
            "id": txn_id,
            "type": "deposit",
            "status": status,
            "deposit_address": address,
            "how": how,
        });

        // Property: Parsing should never panic
        let serialized = serde_json::to_string(&response)
            .expect("must serialize");
        let _reparsed: Value = serde_json::from_str(&serialized)
            .expect("must deserialize");
    });
}
```

### When to Add Property Tests

Add property-based tests when:
- The function processes structured input with many valid variations
- There are invariants that should hold for ALL valid inputs
- Edge cases are hard to enumerate manually
- You want to verify round-trip behavior (serialize → deserialize)

### Regression fixtures

When adding a parser regression, prefer the existing mock fixture pattern in [`src/mock.rs`](src/mock.rs) and the representative tests in [`tests/sep_fixtures_tests.rs`](tests/sep_fixtures_tests.rs).

```bash
cargo test --features mock-only test_sep6_deposit_minimal_fields
```

Keep fixtures minimal and synthetic: reuse the same mock style, include only the fields required to trigger the bug, and avoid real user IDs, credentials, or network data. If the parser issue is about an edge case, construct the smallest valid JSON payload that reproduces it instead of copying a large production response.

### Proptest Documentation

- [proptest Guide](https://docs.rs/proptest/latest/proptest/)
- [Strategy Documentation](https://docs.rs/proptest/latest/proptest/strategy/trait.Strategy.html)

---

## Fuzzing (#644)

Fuzzing feeds randomized data to parsers and validates they handle invalid input without crashing or entering undefined behavior.

### Fuzz Targets

Located in `fuzz/fuzz_targets/`:

1. **json_response_parsing** — Tests JSON parsing robustness
2. **config_parsing** — Tests configuration file parsing (JSON/TOML)
3. **jwt_parsing** — Tests JWT component parsing and base64url decoding
4. **domain_validation** — Tests domain validation against arbitrary input

### Running Fuzzing

```bash
# Install cargo-fuzz (one-time)
cargo install cargo-fuzz

# Run a specific fuzz target
cd fuzz
cargo fuzz run fuzz_target_json_response_parsing

# Run with specific options
cargo fuzz run fuzz_target_jwt_parsing -- -max_len=4096 -runs=100000

# Run until a crash is found (will interrupt)
cargo fuzz run fuzz_target_config_parsing

# Run all fuzz targets
for target in fuzz_target_*; do
    echo "Fuzzing $target..."
    cargo fuzz run "$target" -- -max_len=8192 -runs=50000
done
```

### Interpreting Fuzzing Results

When a crash is found, libfuzzer creates an artifact:

```
fuzz/artifacts/fuzz_target_json_response_parsing/<hash>
```

### Steps to Fix a Fuzzing Crash

1. **Identify the crashing input**
   ```bash
   cargo fuzz run fuzz_target_json_response_parsing -- fuzz/artifacts/fuzz_target_json_response_parsing/<hash>
   ```

2. **Add a regression test**
   ```rust
   #[test]
   fn regression_fuzzing_crash_xyz() {
       let input = b"<crashing input>";
       // Verify parser handles it gracefully
   }
   ```

3. **Fix the underlying bug**
   - Ensure the parser handles all inputs without panicking
   - Use proper error handling instead of unwrap()

4. **Verify the fix**
   ```bash
   cargo fuzz run fuzz_target_json_response_parsing
   ```

### Corpus and Artifacts

- **Corpus**: `fuzz/corpus/fuzz_target_*/` — Interesting inputs discovered during fuzzing
- **Artifacts**: `fuzz/artifacts/fuzz_target_*/` — Inputs that caused crashes

Commit the corpus to git to preserve coverage gains across team members.

### CI Integration

Add to your CI pipeline:

```bash
#!/bin/bash
cd fuzz
for target in fuzz_target_*; do
    cargo fuzz run "$target" -- -max_len=8192 -runs=100000 || exit 1
done
```

---

## Feature-Flag Matrix Testing (#642)

The AnchorKit project supports multiple build configurations. The feature-flag matrix test suite ensures all combinations compile and work correctly.

### Supported Configurations

| Configuration | Command | Use Case |
|---|---|---|
| **Default (std)** | `cargo build --release` | Native servers and CLIs |
| **Explicit std** | `cargo build --release --features std` | Native builds with explicit feature |
| **std + mock-only** | `cargo build --features std,mock-only` | Testing with fixtures |
| **std + stress-tests** | `cargo build --features std,stress-tests` | Load testing |
| **mock-only** | `cargo build --no-default-features --features mock-only` | Test fixtures only |
| **stress-tests** | `cargo build --no-default-features --features stress-tests` | Load testing without std |
| **WASM** | `cargo build --target wasm32-unknown-unknown --no-default-features --features wasm` | On-chain Soroban deployment |

### Running Feature Matrix Tests

```bash
# Run quick compilation checks for all combinations
./scripts/test_feature_matrix.sh --quick

# Run full tests (including cargo test for each combination)
./scripts/test_feature_matrix.sh --full

# Run matrix tests from cargo
cargo test --test feature_flag_matrix_tests --release

# Test a specific configuration
cargo build --release --features std,mock-only
cargo test --features std,mock-only
```

### Feature Constraints

| Feature | Incompatible With | Reason |
|---|---|---|
| **std** | wasm | Mutually exclusive (std requires native runtime) |
| **wasm** | std | Requires --no-default-features |
| **mock-only** | (none) | Safe to combine with any other feature |
| **stress-tests** | (none) | Safe to combine with any other feature |

### Feature Documentation

Each feature flag in `Cargo.toml` has clear documentation:

```toml
# std: enables standard library support (networking, filesystem, threads).
#      Enabled by default for native builds. Incompatible with wasm feature.
std = ["clap", "reqwest", "aes-gcm", "argon2", "rpassword", "rand/std"]

# wasm: targets wasm32-unknown-unknown for Soroban on-chain deployment.
#       Disables std, CLI, and any host-only modules (main.rs, examples).
#       Must be built with --no-default-features.
wasm = []

# mock-only — Pre-built response fixtures for testing without a live anchor.
mock-only = []

# stress-tests — Load-simulation integration test suite.
stress-tests = []
```

### Matrix Test Invariants

The feature_flag_matrix_tests.rs file verifies:

1. ✓ Default features are set correctly
2. ✓ std and wasm are mutually exclusive
3. ✓ mock-only is optional and available with all valid combinations
4. ✓ stress-tests is optional and available with all valid combinations
5. ✓ mock-only has no production use warnings

### Adding New Features

When adding a new feature flag:

1. Add to `Cargo.toml` with clear documentation
2. Add invariant tests in `feature_flag_matrix_tests.rs`
3. Update the feature matrix script in `scripts/test_feature_matrix.sh`
4. Add example build commands to the test file

---

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: Test Suite

on: [push, pull_request]

jobs:
  property-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --test property_based_parsing_tests

  feature-matrix:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: ./scripts/test_feature_matrix.sh --quick

  fuzzing:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@nightly
      - run: cargo install cargo-fuzz
      - run: cd fuzz && for t in fuzz_target_*; do cargo fuzz run "$t" -- -max_len=4096 -runs=10000 || exit 1; done
```

---

## Best Practices

### For New Parsing/Validation Code

1. **Add property-based tests** when introducing new parsers
   - Define valid input strategies
   - Verify invariants hold across all generated inputs

2. **Add fuzz targets** for public parsing APIs
   - Create fuzz target in `fuzz/fuzz_targets/`
   - Ensure parser never panics on arbitrary input

3. **Update feature matrix** if the code is feature-gated
   - Add tests to `feature_flag_matrix_tests.rs`
   - Verify compilation for all configurations

### For Existing Code

1. **Property tests complement unit tests**
   - Unit tests: specific known cases
   - Property tests: general invariants and edge cases

2. **Fuzzing finds crashes and panics**
   - Run periodically in CI
   - Fix all crashes with regression tests

3. **Feature matrix ensures consistency**
   - Run before shipping new features
   - Prevents accidental feature conflicts

---

## Troubleshooting

### Property Test Failures

**Issue**: Test fails with "Strategy value generation"
**Solution**: Reduce `PROPTEST_CASES` or adjust strategy bounds

```bash
PROPTEST_CASES=100 cargo test --test property_based_parsing_tests
```

### Fuzzing Timeouts

**Issue**: Fuzzing runs too long or finds timeout crashes
**Solution**: Reduce max input length or increase timeout

```bash
cargo fuzz run fuzz_target_json_response_parsing -- -max_len=1024 -timeout=5
```

### Feature Compilation Errors

**Issue**: Feature combination doesn't compile
**Solution**: Check feature gate guards and dependencies

```bash
cargo check --no-default-features --features wasm
```

---

## References

- [proptest Documentation](https://docs.rs/proptest/)
- [cargo-fuzz Guide](https://rust-fuzz.github.io/book/cargo-fuzz.html)
- [LLVM libfuzzer](https://llvm.org/docs/LibFuzzer/)
- [Rust Book - Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)

---

## Contact & Support

For issues or questions about the testing infrastructure:
1. Check this guide and linked documentation
2. Review existing test examples
3. Open an issue with `[testing]` tag
one negative test proving a zero or over-limit page size has explicit, bounded behavior, plus a regression test proving normal pages are untouched.

 `<angle_brackets>` must be replaced with real names from the contract. Where the contract doesn't answer a question, this spec says to flag it rather than invent a limit.

---

## 0. Scope and Rules

**In scope**
- One negative test for a single invalid page size (zero **or** over-limit).
- One regression test for a normal page size.
- Running the attestation pagination tests.

**Out of scope**
- Adding a new max constant or limit unless the contract interface already defines one.
- Changing the pagination signature.
- Refactoring storage or indexing.

**Hard rules**
1. Inspect before writing. The contract decides the expected behavior, not this document.
2. Use the contract's own constant for boundaries. Never hardcode a number the contract could change.
3. Assert either a stable error **or** documented clamping. Never assert "it doesn't crash."
4. If no bound exists today, stop and report it. That is a finding, not a test to fake.

---

## 1. Inspect the Current Contract

### 1.1 Find the pagination entrypoint

```bash
cd <repo-root>
grep -rn "fn list_\|fn get_.*attest\|fn .*page\|fn .*paginat\|fn query_" \
  contracts/attestation/src
```

Look for functions with a shape like:

```rust
pub fn list_attestations(
    env: Env,
    subject: Address,
    start: u32,      // offset or cursor
    limit: u32,      // THE size argument
) -> Vec<Attestation>
```

Record the answers:

| Question | Answer (fill in) |
|---|---|
| Function name | `<list_fn>` |
| Size arg name | `<limit \| page_size \| count>` |
| Size arg type | `<u32 \| u64>` |
| Start arg style | `<offset \| cursor \| page_number>` |
| Return type | `<Vec<T> \| Page<T>>` |
| Return includes next cursor? | `<yes/no>` |

### 1.2 Find any published maximum

```bash
grep -rn "MAX_\|max_page\|MAX_LIMIT\|PAGE_SIZE\|DEFAULT_LIMIT" \
  contracts/attestation/src
grep -rn -i "page size\|limit\|maximum\|pagination" \
  contracts/attestation/README.md docs/ 2>/dev/null
```

Check three places, since "published" can mean any of them:
- A `const` in the contract source.
- The doc comment on the function.
- The README, interface docs, or a generated spec.

| Item | Found? | Value | Location |
|---|---|---|---|
| Max page size const | | | |
| Default page size const | | | |
| Documented in README/interface | | | |
| Doc comment on function | | | |

### 1.3 Find the current validation behavior

Read the function body and note what happens for each input:

```rust
// Pattern A: explicit error
if limit == 0 || limit > MAX_PAGE_SIZE {
    return Err(Error::InvalidPageSize);
}

// Pattern B: clamp
let limit = limit.min(MAX_PAGE_SIZE);

// Pattern C: clamp with zero default
let limit = if limit == 0 { DEFAULT_PAGE_SIZE } else { limit.min(MAX_PAGE_SIZE) };

// Pattern D: panic
assert!(limit > 0 && limit <= MAX_PAGE_SIZE, "bad limit");

// Pattern E: nothing at all (unbounded)
for i in start..(start + limit) { ... }
```

Fill in the behavior table:

| Input | Current behavior | Pattern |
|---|---|---|
| `limit = 0` | | |
| `limit = MAX` | | |
| `limit = MAX + 1` | | |
| `limit = u32::MAX` | | |
| `start` beyond total | | |
| empty collection | | |

### 1.4 Check for existing tests

```bash
grep -rn "pagination\|limit\|page" contracts/attestation/src/test*.rs \
  contracts/attestation/tests 2>/dev/null
ls contracts/attestation/src | grep -i test
```

Note the test file location, the helper names (setup functions, fixtures), and whether a pagination test already exists. Extend that file. Don't create a parallel one.

### 1.5 Check the contract interface

If the project publishes an interface (spec JSON, TypeScript bindings, docs):

```bash
stellar contract bindings typescript --help   # if bindings are generated
grep -rn "list_\|limit" bindings/ 2>/dev/null
```

Rule: **a change that alters the published interface needs explicit sign-off.** A test alone should not touch it.

---

## 2. Decide Which Behavior to Test

Use this matrix after Section 1.

| Situation found | Test to write | Expected assertion |
|---|---|---|
| Error exists for over-limit | Over-limit test | Specific error variant |
| Error exists for zero only | Zero test | Specific error variant |
| Clamp exists, documented | Over-limit test | `len == MAX` |
| Clamp exists, undocumented | Report; test clamp only if the team confirms it as the contract | `len == MAX` |
| Panic on invalid | Over-limit test with `try_` call | Contract error / `Err` |
| **No bound at all** | **Do not write a passing test.** Flag it | Open a decision: error vs clamp |

### 2.1 Pick exactly ONE invalid case

The task says "one invalid size." Choose:

- **Max constant exists** → test `MAX_PAGE_SIZE + 1`.
- **Only a zero guard exists** → test `0`.
- **Both exist** → pick over-limit, since it directly proves the response is bounded.

### 2.2 If there is no bound

Write this up instead of a test:

```
FINDING: <list_fn> accepts any u32 for <limit>. With N attestations stored,
a request of u32::MAX attempts to return all N records in one call.
No published maximum exists in source, docs, or interface.

DECISION NEEDED (owner of contract interface):
  Option 1: reject with Error::InvalidPageSize for 0 or > MAX
  Option 2: clamp to MAX, document it
  
This task does not add a limit. Test deferred until the contract defines one.
```

That satisfies the acceptance criterion "no new limit is added without checking the contract interface."

---

## 3. Choose the Boundary Value

Boundaries come from the contract, not from guesses.

```rust
use crate::{MAX_PAGE_SIZE, Error};          // adjust paths
// or, if the const is private:
// use crate::constants::MAX_PAGE_SIZE;
```

| Case | Value | Why |
|---|---|---|
| Zero | `0` | Degenerate size; often an off-by-one trap |
| Just over | `MAX_PAGE_SIZE + 1` | The tightest violation |
| Exactly at max | `MAX_PAGE_SIZE` | Must still succeed |
| Just under | `MAX_PAGE_SIZE - 1` | Must still succeed |
| Extreme | `u32::MAX` | Catches overflow and unbounded loops |

**Overflow caution:** if `MAX_PAGE_SIZE` could be `u32::MAX`, then `MAX + 1` wraps. Use `checked_add` or test `u32::MAX` directly:

```rust
let over = MAX_PAGE_SIZE.checked_add(1).expect("max is u32::MAX; use a different invalid case");
```

If the const is private and the test crate can't import it, ask whether it should be exposed to tests with `pub(crate)` or `#[cfg(test)]`. Don't copy the literal.

---

## 4. Test Harness

Reuse existing helpers if the test module has them. These are the minimum shapes needed.

### 4.1 Imports

```rust
#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

use crate::{
    <ContractName>, <ContractName>Client, Error, MAX_PAGE_SIZE,
    // plus the Attestation type, if needed
};
```

### 4.2 Setup helper

```rust
struct Fixture<'a> {
    env: Env,
    client: <ContractName>Client<'a>,
    subject: Address,
    issuer: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(<ContractName>, ());
    let client = <ContractName>Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let subject = Address::generate(&env);

    // Only if the contract needs init / issuer registration:
    // client.initialize(&admin);
    // client.register_issuer(&issuer);

    Fixture { env, client, subject, issuer }
}
```

### 4.3 Seeding helper

Seed a known number of records so assertions are exact.

```rust
fn seed(f: &Fixture, n: u32) {
    for i in 0..n {
        // Use the REAL attest/create function and signature.
        f.client.<attest_fn>(
            &f.issuer,
            &f.subject,
            &String::from_str(&f.env, &format!("claim-{}", i)),
            // ...other required args
        );
    }
}
```

Notes:
- `format!` isn't available in `no_std` contract code, but it **is** available in test code when the `std` feature is enabled for tests. If it isn't, build the string from a fixed set or use a counter-derived `BytesN`.
- Soroban test budgets are finite. If seeding `MAX_PAGE_SIZE + 10` records exhausts the budget, call `env.cost_estimate().budget().reset_unlimited()` in the test, and only there.

```rust
f.env.cost_estimate().budget().reset_unlimited();
```

### 4.4 Large-seed guard

```rust
// Seeding cost scales with MAX_PAGE_SIZE. If MAX is large (e.g. 1000),
// keep the dataset to MAX + small margin, not far above.
const SEED_MARGIN: u32 = 5;
```

---

## 5. The Negative Test

Write **one** of 5A or 5B, depending on Section 2.

### 5A. Contract returns a stable error

```rust
#[test]
fn pagination_rejects_over_limit_page_size() {
    let f = setup();
    seed(&f, 5);

    let over_limit = MAX_PAGE_SIZE + 1;
    let res = f.client.try_<list_fn>(&f.subject, &0u32, &over_limit);

    // Stable error: assert the exact variant, not just is_err().
    assert_eq!(res, Err(Ok(Error::InvalidPageSize)));
}
```

**Why this shape**
- `try_` returns `Result<Result<T, ConversionError>, Result<Error, InvokeError>>`. The outer `Err(Ok(Error::X))` is a contract-defined error. Adjust the nesting if your SDK version differs.
- Asserting the exact variant makes the error part of the contract. A refactor that changes the variant fails the test.
- The seeded count (5) is deliberately small. The point is that the request is rejected regardless of how many records exist.

### 5A-zero. Zero variant

Use this instead if only zero is guarded:

```rust
#[test]
fn pagination_rejects_zero_page_size() {
    let f = setup();
    seed(&f, 5);

    let res = f.client.try_<list_fn>(&f.subject, &0u32, &0u32);

    assert_eq!(res, Err(Ok(Error::InvalidPageSize)));
}
```

### 5B. Contract documents clamping

```rust
#[test]
fn pagination_clamps_over_limit_page_size_to_max() {
    let f = setup();
    f.env.cost_estimate().budget().reset_unlimited();
    seed(&f, MAX_PAGE_SIZE + SEED_MARGIN);

    let page = f.client.<list_fn>(&f.subject, &0u32, &(MAX_PAGE_SIZE + 1));

    // Bounded: exactly MAX, not total, not MAX+1.
    assert_eq!(page.len(), MAX_PAGE_SIZE);
}
```

### 5B-zero. Clamp-to-default variant

```rust
#[test]
fn pagination_zero_page_size_uses_default() {
    let f = setup();
    seed(&f, DEFAULT_PAGE_SIZE + SEED_MARGIN);

    let page = f.client.<list_fn>(&f.subject, &0u32, &0u32);

    assert_eq!(page.len(), DEFAULT_PAGE_SIZE);
}
```

### 5.1 What "stable" means

A stable error is:
- A named `contract_error` variant with a fixed numeric code.
- Not a string-matched panic message.
- Not dependent on dataset size.

Check the error enum:

```rust
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    // ...
    InvalidPageSize = <n>,   // numeric value is part of the contract
}
```

If the contract panics with a string instead of returning an error, assert that in a `#[should_panic(expected = "...")]` test **only** if that panic is the documented behavior. Then note in the PR that a typed error would be more stable. Don't change it in this task.

---

## 6. The Regression Test (Normal Page Size)

This proves the validation didn't break the happy path.

```rust
#[test]
fn pagination_normal_page_size_returns_expected_records() {
    let f = setup();
    seed(&f, 5);

    let page = f.client.<list_fn>(&f.subject, &0u32, &3u32);

    assert_eq!(page.len(), 3);

    // Check content and order, not only the count.
    for i in 0..3u32 {
        let a = page.get(i).unwrap();
        assert_eq!(a.subject, f.subject);
        // assert_eq!(a.id, <expected_id_for_i>);
    }
}
```

### 6.1 Second page

```rust
#[test]
fn pagination_second_page_continues_after_first() {
    let f = setup();
    seed(&f, 5);

    let p1 = f.client.<list_fn>(&f.subject, &0u32, &3u32);
    let p2 = f.client.<list_fn>(&f.subject, &3u32, &3u32); // adjust for cursor style

    assert_eq!(p1.len(), 3);
    assert_eq!(p2.len(), 2);

    // No overlap between pages.
    for a in p1.iter() {
        for b in p2.iter() {
            assert_ne!(a.id, b.id);
        }
    }
}
```

If pagination is cursor-based, replace the second call with the cursor returned from the first page.

### 6.2 Exactly-at-max succeeds

```rust
#[test]
fn pagination_exact_max_page_size_succeeds() {
    let f = setup();
    f.env.cost_estimate().budget().reset_unlimited();
    seed(&f, MAX_PAGE_SIZE + SEED_MARGIN);

    let page = f.client.<list_fn>(&f.subject, &0u32, &MAX_PAGE_SIZE);

    assert_eq!(page.len(), MAX_PAGE_SIZE);
}
```

This guards against an off-by-one where the check is `>=` instead of `>`.

### 6.3 Page size larger than the dataset

```rust
#[test]
fn pagination_page_size_above_total_but_under_max_returns_all() {
    let f = setup();
    seed(&f, 4);

    let page = f.client.<list_fn>(&f.subject, &0u32, &10u32); // 10 <= MAX

    assert_eq!(page.len(), 4);
}
```

---

## 7. Optional Edge Tests

Add these only if cheap and only if the contract behavior is clear. The task asks for one invalid-size negative test, so keep extras in the regression category.

### 7.1 Empty collection

```rust
#[test]
fn pagination_on_empty_set_returns_empty() {
    let f = setup();

    let page = f.client.<list_fn>(&f.subject, &0u32, &5u32);

    assert_eq!(page.len(), 0);
}
```

### 7.2 Start beyond the end

```rust
#[test]
fn pagination_start_beyond_total_returns_empty() {
    let f = setup();
    seed(&f, 3);

    let page = f.client.<list_fn>(&f.subject, &100u32, &5u32);

    assert_eq!(page.len(), 0);
}
```

Confirm the current behavior first. If the contract errors here, assert the error instead. Never assume.

### 7.3 Extreme value

```rust
#[test]
fn pagination_u32_max_page_size_is_bounded() {
    let f = setup();
    seed(&f, 5);

    let res = f.client.try_<list_fn>(&f.subject, &0u32, &u32::MAX);

    // Error contract:
    assert_eq!(res, Err(Ok(Error::InvalidPageSize)));
    // Clamp contract: assert len <= MAX_PAGE_SIZE instead.
}
```

### 7.4 Start + limit overflow

```rust
#[test]
fn pagination_start_plus_limit_does_not_overflow() {
    let f = setup();
    seed(&f, 3);

    // start near u32::MAX with a valid limit must not panic on addition.
    let res = f.client.try_<list_fn>(&f.subject, &(u32::MAX - 1), &5u32);

    assert!(res.is_ok()); // returns empty, never wraps into early indexes
    assert_eq!(res.unwrap().unwrap().len(), 0);
}
```

This catches `start + limit` wrapping into valid indexes, a real source of unbounded or wrong responses.

### 7.5 Other subjects unaffected

```rust
#[test]
fn pagination_only_returns_requested_subjects_records() {
    let f = setup();
    seed(&f, 3);
    let other = Address::generate(&f.env);
    // attest for `other` once
    f.client.<attest_fn>(&f.issuer, &other, &String::from_str(&f.env, "x"));

    let page = f.client.<list_fn>(&f.subject, &0u32, &10u32);

    assert_eq!(page.len(), 3);
}
```

---

## 8. Proving the Response Is Bounded

"Bounded" should be demonstrated, not inferred from `len`.

### 8.1 Length assertion (clamp contracts)

```rust
assert!(page.len() <= MAX_PAGE_SIZE);
```

### 8.2 Budget assertion

If the project tracks resource cost, compare the cost of a max-size page to an invalid request:

```rust
#[test]
fn invalid_page_size_is_rejected_cheaply() {
    let f = setup();
    seed(&f, 5);

    f.env.cost_estimate().budget().reset_default();
    let _ = f.client.try_<list_fn>(&f.subject, &0u32, &(MAX_PAGE_SIZE + 1));
    let cpu = f.env.cost_estimate().budget().cpu_instruction_cost();

    // Rejection should happen before iteration; keep the threshold loose.
    assert!(cpu < <GENEROUS_BOUND>);
}
```

Only include this if the repo already uses budget assertions. A new brittle numeric threshold isn't worth adding. If you include it, set the bound well above observed cost.

### 8.3 Validation order

Validation must happen **before** reading storage:

```rust
// good
if limit == 0 || limit > MAX_PAGE_SIZE { return Err(Error::InvalidPageSize); }
let items = read_storage(...);

// bad: reads everything, then trims
let items = read_storage(...);
if items.len() > MAX_PAGE_SIZE { ... }
```

Review the function body for this. If the order is wrong, report it. That's a contract fix outside this task.

---

## 9. Run the Tests

### 9.1 Targeted

```bash
cargo test -p <attestation-crate> pagination -- --nocapture
```

### 9.2 Full attestation suite

```bash
cargo test -p <attestation-crate>
```

### 9.3 Workspace sanity

```bash
cargo test --workspace
cargo clippy -p <attestation-crate> --all-targets -- -D warnings
cargo fmt --all -- --check
```

### 9.4 Confirm the test actually tests something

Mutation check, done locally and not committed:

1. In the contract, temporarily change `limit > MAX_PAGE_SIZE` to `limit > MAX_PAGE_SIZE + 1`.
2. Run the negative test.
3. It must **fail**.
4. Revert the change.

If it still passes, the boundary value isn't hitting the guard. Fix the test.

Repeat for the zero guard (`limit == 0` → `limit == 1`) if you wrote the zero test.

### 9.5 Expected output

```
running 4 tests
test tests::pagination_rejects_over_limit_page_size ... ok
test tests::pagination_normal_page_size_returns_expected_records ... ok
test tests::pagination_exact_max_page_size_succeeds ... ok
test tests::pagination_second_page_continues_after_first ... ok

test result: ok. 4 passed; 0 failed
```

---

## 10. Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `MAX_PAGE_SIZE` not found | Private const | Ask to expose with `pub(crate)`; don't hardcode |
| `try_` result shape mismatch | SDK version | Match against the existing tests' `try_` handling |
| Budget exceeded while seeding | Large `MAX` | `budget().reset_unlimited()` inside the test only |
| Negative test passes before any guard exists | Contract panics generically | Inspect the failure; assert the real error or report the gap |
| Negative test passes with the guard mutated | Wrong boundary value | Use `MAX + 1`, not an arbitrary large number |
| `MAX + 1` overflows | `MAX == u32::MAX` | Use `checked_add` or test a different case |
| Order assertions flaky | Storage iteration unordered | Assert as a set, or sort by id before comparing |
| Test passes locally, fails in CI | Different feature flags | Run with the CI command and flags |
| Seeded records leak between tests | Shared env | Each test calls `setup()` for a fresh `Env` |

---

## 11. Documentation Rules

Update docs **only** to reflect behavior that already exists.

- Error contract exists but isn't documented → add one line to the function doc comment and the README naming the error and the max.
- Clamp exists but isn't documented → document it, and tell the team the behavior is now stated.
- No bound exists → **change no docs.** Report the finding (Section 2.2).

Suggested doc comment, if the behavior already exists:

```rust
/// Returns up to `limit` attestations for `subject`, starting at `start`.
///
/// # Errors
/// * `Error::InvalidPageSize` if `limit == 0` or `limit > MAX_PAGE_SIZE`.
///
/// `MAX_PAGE_SIZE` is <value>. Callers needing more records should page.
pub fn <list_fn>(...)
```

---

## 12. PR Description Template

```markdown
## Summary
Adds a negative test for an invalid pagination size on `<list_fn>`, plus a
regression test for a normal page size.

## What I checked
- Pagination entrypoint: `<list_fn>(subject, start, limit)`
- Published maximum: `MAX_PAGE_SIZE = <n>` in `<file>` (also documented in <README/none>)
- Current behavior: <rejects with Error::InvalidPageSize | clamps to MAX>

## Tests added
- `pagination_rejects_over_limit_page_size`: `MAX_PAGE_SIZE + 1` → `Error::InvalidPageSize`
- `pagination_normal_page_size_returns_expected_records`: limit 3 of 5 → 3 records, correct order
- `pagination_exact_max_page_size_succeeds`: boundary still accepted

## Not changed
- No new limit, constant, or error variant was added.
- Contract interface and README unchanged.

## Verification
- `cargo test -p <crate> pagination` → all pass
- Mutation check: loosening the guard by one makes the negative test fail
- `cargo clippy` and `cargo fmt --check` clean
```

If a finding was reported instead:

```markdown
## Summary
No test added. `<list_fn>` has no page-size bound (see finding below).
Per task criteria, no limit was added without interface review.

## Finding
<paste Section 2.2 text>
```

---

## 13. Acceptance Checklist

**Explicit and bounded**
- [ ] One invalid size (zero or over-limit) is covered by a test.
- [ ] The test asserts a stable error variant **or** the documented clamp length.
- [ ] The boundary value comes from the contract constant, not a literal.
- [ ] The result is demonstrably bounded (`len <= MAX` or rejected before iteration).

**Normal behavior preserved**
- [ ] A normal page size returns the expected records, in order.
- [ ] The exact-max size still succeeds.
- [ ] The second page continues without overlap.

**Contract discipline**

- [ ] No new limit or constant added without checking the interface.
- [ ] No signature, error code number, or event changed.
- [ ] Docs updated only if the behavior already existed.
- [ ] If no bound existed, a finding was filed instead of a test.

**Process**
- [ ] Attestation pagination tests run and pass.
- [ ] Full attestation suite passes.
- [ ] Mutation check proves the test can fail.
- [ ] `clippy` and `fmt` clean.

---

## 14. Decision Summary

```
Inspect function and constants
        │
        ├── Bound exists, returns error ──► test MAX+1 (or 0), assert exact Error
        │
        ├── Bound exists, clamps ─────────► test MAX+1, assert len == MAX
        │
        └── No bound ─────────────────────► STOP. File finding. No test. No new limit.

Always add: normal-size regression test (+ exact-max test)
Always run: pagination tests, then the full attestation suite
Always confirm: the negative test fails when the guard is loosened
```

---

If you paste the pagination function, the max constant, and the existing test module's setup helpers, I'll replace every `<placeholder>` and give you the final test code as-is.
