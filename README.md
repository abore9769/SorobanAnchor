# SorobanAnchor

A Soroban smart contract SDK for Stellar anchors. Handles attestation management, SEP-6 deposit/withdrawal flows, SEP-10 JWT authentication, anchor routing, rate limiting, and transaction state tracking — all in a `no_std` Rust library compiled to WASM.

## What it does

- Registers and revokes attestors with SEP-10 JWT verification
- Submits and retrieves on-chain attestations with replay attack protection
- Normalizes SEP-6 deposit, withdrawal, and transaction status responses across anchors
- Verifies SEP-10 EdDSA JWTs on-chain using stored Ed25519 public keys
- Routes requests across multiple anchors and asset pairs by reputation, fees, and settlement time
- Caches anchor metadata and stellar.toml capabilities with TTL-based expiry
- Tracks transaction state transitions with full audit logging
- Propagates request IDs and tracing spans across operations
- Enforces rate limits and configurable retry/backoff strategies
- Validates anchor domain endpoints and response schemas

## Project structure

```
src/                        # Core library
  lib.rs                    # Public API surface
  contract.rs               # Soroban contract (attestations, sessions, quotes, routing)
  sep6.rs                   # SEP-6 deposit/withdrawal normalization
  sep10_jwt.rs              # SEP-10 JWT verification (EdDSA, no_std)
  domain_validator.rs       # Anchor domain/endpoint validation
  errors.rs                 # Stable error codes
  rate_limiter.rs           # Rate limiting
  response_validator.rs     # Response schema validation
  retry.rs                  # Retry with exponential backoff
  transaction_state_tracker.rs
  multi_asset_routing.rs    # Multi-asset quote routing across corridors
  deterministic_hash.rs     # Canonical SHA-256 payload hashing

tests/                      # Integration and unit tests
configs/                    # Example anchor configurations (JSON + TOML)
examples/                   # Rust and shell usage examples
scripts/                    # Build, validation, and deploy scripts
docs/                       # Feature and guide documentation
test_snapshots/             # Snapshot fixtures for deterministic tests
```

## Building

```bash
cargo build --release
```

For WASM output (Soroban deployment):

```bash
cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm
```

Generate browsable API reference docs for the public crate exports:

```bash
make api-docs
```

The generated HTML is written to `target/api-docs/doc/anchorkit/index.html`.

### Build Matrix

SorobanAnchor supports two distinct build environments with complete feature separation:

| Configuration | Command | Target | Output | CLI |
|---|---|---|---|---|
| **Native (default)** | `cargo build --release` | `x86_64-unknown-linux-gnu` | `target/release/anchorkit` | ✓ Yes |
| **WASM/Soroban** | `cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm` | `wasm32-unknown-unknown` | `target/wasm32-unknown-unknown/release/anchorkit.wasm` | ✗ No |

**Key differences:**
- **Native**: Includes CLI, HTTP client (`reqwest`), filesystem access, and credential storage
- **WASM**: Minimal runtime, no std library, only smart contract code for Soroban

Both builds are verified by the automated test suite:

```bash
./scripts/test_build_matrix.sh
```

For detailed information about build paths, features, and environment separation, see [docs/build-matrix.md](docs/build-matrix.md).

## Testing

```bash
cargo test
```

Run the stress-test suite (excluded from normal CI):

```bash
cargo test --features stress-tests
```

## Feature flags

The crate uses four feature flags to control which modules are compiled.

| Flag | Default | Purpose |
|------|---------|---------|
| `std` | ✓ | Enables filesystem-based config loading (`load_runtime_config_file`, `RuntimeConfig`). Disable for pure no_std environments. |
| `wasm` | — | Soroban on-chain deployment target. Excludes all HTTP/host modules (`sep6`, `sep24`, `sep38`, `webhook`, `streaming_monitor`); only the contract, error types, rate limiter, and cryptographic utilities are compiled. |
| `mock-only` | — | Enables the `mock` module with pre-built valid fixtures for every response type. Use in integration tests and CI pipelines that have no live anchor. |
| `stress-tests` | — | Enables `tests/load_simulation_tests.rs` — high-concurrency and throughput tests excluded from normal CI. |

### Build variants

```bash
# Native development (default features)
cargo build

# Soroban on-chain WASM deployment
cargo build --release \
  --target wasm32-unknown-unknown \
  --no-default-features --features wasm

# Testing with mock fixtures (no live anchor)
cargo test --features mock-only

# Testing with mock fixtures and config (std + mock)
cargo test --features std,mock-only

# Full suite including stress tests
cargo test --features std,mock-only,stress-tests

# Library only, no std (no_std verification)
cargo check --no-default-features
```

### Using mock fixtures

```rust
use anchorkit::mock::{mock_deposit_response, mock_firm_quote};
use anchorkit::{initiate_deposit, sep38::request_firm_quote};

// Test the deposit parsing pipeline without a live anchor
let raw = mock_deposit_response();
let deposit = initiate_deposit(raw).unwrap();
assert_eq!(deposit.transaction_id, "mock-txn-001");

// Test SEP-38 quote parsing
let raw_quote = mock_firm_quote();
let quote = request_firm_quote(raw_quote, 1_700_000_000).unwrap();
assert!(!quote.id.is_empty());
```

## CLI

```bash
# Check environment setup before doing anything else
anchorkit doctor

# Validate config files offline (no network access required)
anchorkit offline validate

# Deploy to testnet
anchorkit deploy --network testnet

# Deploy to mainnet (prompts for confirmation unless --yes is passed)
anchorkit deploy --network mainnet --yes

# Register an attestor
anchorkit register --address GANCHOR123... --services deposits,withdrawals,kyc

# Submit an attestation
anchorkit attest --subject GUSER123... --payload-hash abc123...

# Verify an attestation and check contract/rate-limiter health
anchorkit verify --id 42
anchorkit health --contract-id $ANCHOR_CONTRACT_ID --attestor GANCHOR123...
```

**Walkthroughs and troubleshooting:** see [`examples/full_deployment_walkthrough.sh`](examples/full_deployment_walkthrough.sh)
for an end-to-end operator tour (doctor → validate → deploy → register →
attest → verify → health) with a troubleshooting section for common errors.
Other scenario walkthroughs live in [`examples/`](examples/):

| Example | Covers |
|---|---|
| `full_deployment_walkthrough.sh` | End-to-end deploy → register → attest → verify, plus troubleshooting |
| `attestation_workflow.sh` | Registration, plain/session/traced attestations, replay protection |
| `rate_limit_override_example.sh` | Per-role and per-attestor rate limit overrides |
| `config_hot_reload_example.rs` | Reloading runtime config without restarting the process |
| `credential_management.sh` | Encrypted keystore vs. env var vs. keypair file |
| `mock_mode_example.sh` | Testing CLI flows without a live anchor |
| `offline_mode_example.sh` | Config validation and workflow simulation with no network |
| `anchor_info_discovery.sh` | Fetching and interpreting a `stellar.toml` |
| `kyc_workflow.sh` | KYC-gated attestation flow |
| `role_usage_example.sh` | Admin roles and capability delegation |

## Supported SEP Versions

The contract explicitly exposes which Stellar Ecosystem Proposals (SEPs) it supports via two on-chain methods:

```rust
// Returns [6, 10, 24, 31, 38]
let seps: Vec<u32> = AnchorKitContract::supported_seps(env);

// Returns per-SEP boolean flags
let flags: SepFeatureFlags = AnchorKitContract::supported_sep_feature_flags(env);
assert!(flags.sep10); // SEP-10 JWT authentication is supported
```

| SEP | Description | Supported |
|-----|-------------|-----------|
| [SEP-6](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0006.md) | Non-interactive deposit and withdrawal | ✓ |
| [SEP-10](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0010.md) | Stellar Web Authentication (JWT) | ✓ |
| [SEP-24](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0024.md) | Interactive deposit and withdrawal | ✓ |
| [SEP-31](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0031.md) | Direct payment | ✓ |
| [SEP-38](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0038.md) | Anchor RFQ / firm quotes | ✓ |

Constants are also exported from the `contract` module: `SEP_6`, `SEP_10`, `SEP_24`, `SEP_31`, `SEP_38`.

## Key APIs

```rust
// SEP-6: normalize a raw anchor deposit response
let response = initiate_deposit(raw)?;

// SEP-10: verify an anchor JWT on-chain
contract.verify_sep10_token(token, issuer);

// Submit an attestation (replay-protected)
let id = contract.submit_attestation(issuer, subject, timestamp, payload_hash, sig);

// Route across anchors by lowest fee
let best = contract.route(options);

// Route across multiple asset pairs (multi-asset, #656)
use anchorkit::multi_asset_routing::{route_multi_asset, AssetPairRequest};
let requests = vec![
    AssetPairRequest { base_asset: "XLM".into(), quote_asset: "USDC".into(), amount: 1000, strategy: "LowestFee".into(), min_reputation: 70 },
];
let result = route_multi_asset(&requests, &all_quotes, now_timestamp)?;

// Track transaction state
tracker.transition(tx_id, TransactionStatus::Completed);
```

## Configuration

Anchor configs live in `configs/` as JSON or TOML. Validate them with:

```bash
./scripts/validate_all.sh
```

Schema reference: `config_schema.json`

### Strict unknown-field rejection

The schema sets `additionalProperties: false` on every object section. Any key
that is not explicitly listed in the schema causes validation to fail
immediately. This catches typos in field names before a misconfigured anchor
reaches production.

For example, a misspelled top-level key is rejected with an actionable path:

```json
{
  "contract": { "name": "my-anchor", "version": "1.0.0", "network": "stellar-testnet" },
  "attestors": { "registry": [ ... ] },
  "monitorring": {}
}
```

```
schema validation failed:
(root): [additionalProperties] Additional properties are not allowed ('monitorring' was unexpected)
```

The fix is to correct the spelling to `monitoring`. Unknown keys are never
silently ignored — if you need environment-specific values that should not be
validated, keep them in a separate file (e.g. a `.env` file or a CI secret)
and reference them by environment variable rather than embedding them in the
anchor config.

### Runtime config hot-reload

Long-running host processes (not the on-chain contract, which already
applies admin updates immediately) can pick up config file changes without
restarting, via `RuntimeConfigManager`:

```rust
use anchorkit::config::RuntimeConfigManager;

let manager = RuntimeConfigManager::new("configs/remittance-anchor.toml")?;
// ... later, on a timer or file-watch event ...
match manager.reload_if_changed() {
    Ok(true)  => println!("config reloaded"),
    Ok(false) => {} // unchanged
    Err(e)    => eprintln!("reload rejected, previous config kept: {e}"),
}
let current = manager.current();
```

A reload that fails to parse or fails shape validation is rejected and the
previously loaded configuration stays active. See
[`examples/config_hot_reload_example.rs`](examples/config_hot_reload_example.rs).

## Integration Testing

The repository ships a CLI integration test harness that exercises the full
deploy → initialize → register → attest → verify workflow using the Soroban
local simulation environment (no network required by default).

```bash
# Run all integration harness tests (local simulation)
cargo test --test cli_integration_harness

# Or via Make
make integration-test
```

The harness covers:

| Step | What is tested |
|------|---------------|
| 1 | Contract deployment and admin initialization |
| 2 | Attestor registration (SEP-10 JWT flow) |
| 3 | Service capability configuration |
| 4 | Attestation submission and retrieval |
| 5 | Session-based workflow with audit logging |
| 6 | Quote submission and LowestFee routing |
| 7 | Attestor revocation and cleanup |
| E2E | Full pipeline in a single test |
| 9 | KYC submit → approve workflow |
| 10 | CLI binary smoke tests (`doctor`, `deploy --dry-run`) |
| 11 | Live testnet smoke test (opt-in) |

### Live testnet tests

Set the following environment variables to run the live testnet step:

```bash
export SOROBAN_ANCHOR_INTEGRATION=testnet
export ANCHOR_CONTRACT_ID=<deployed-contract-id>
export ANCHOR_ADMIN_SECRET=<admin-secret-key>

make integration-test-live
```

## Release Packaging

Production releases are built and bundled with a single Make target:

```bash
make release
```

This runs `scripts/package_release.sh` which:

1. Ensures the `wasm32-unknown-unknown` Rust target is installed.
2. Builds the native CLI binary (`target/release/anchorkit`).
3. Builds the optimized WASM contract (`target/wasm32-unknown-unknown/release/anchorkit.wasm`).
4. Runs `wasm-opt -Oz` if binaryen is available.
5. Assembles a bundle directory under `dist/anchorkit-<VERSION>/` containing:
   - `anchorkit` — CLI binary
   - `anchorkit.wasm` — Soroban WASM contract
   - `schemas/config_schema.json` — JSON schema for anchor configs
   - `configs/` — Example anchor configurations (JSON + TOML)
   - `docs/` — Documentation
   - `README.md`, `LICENSE`, `VERSION`
6. Creates `dist/anchorkit-<VERSION>.tar.gz`.
7. Generates a SHA-256 checksum file.

### Validating the bundle

```bash
make release-validate
# or directly:
./scripts/validate_bundle.sh dist/anchorkit-0.1.0.tar.gz
```

The validation script checks that all required artifacts are present and that
JSON files are well-formed.

### Signing and verifying

Release artifacts can be signed with GPG or minisign and verified before use:

```bash
# Sign (GPG by default)
make release-sign

# Sign in dry-run mode (prints commands without executing)
make release-sign-dry-run

# Verify a downloaded release bundle
make release-verify TARBALL=dist/anchorkit-0.1.0.tar.gz
# or directly:
./scripts/verify_release.sh dist/anchorkit-0.1.0.tar.gz
```

The verify script checks the SHA-256 checksum, the detached signature, and the
bundle contents.  See [`docs/release-signing.md`](docs/release-signing.md) for
full key-management and CI integration guidance.

### Cleaning up

```bash
make clean-dist   # removes dist/
```

## Governance and Security

SorobanAnchor follows a documented governance and security model covering:

- **Roles** — Maintainers, Contributors, Security Reviewers, and on-chain Attestors.
- **Contract upgrades** — Require two maintainer approvals, a reproducible WASM build, and a published SHA-256 checksum. Only the admin address recorded at contract initialization may authorize upgrades.
- **Admin key management** — Multi-signature setup (2-of-N); keys are never committed to the repository; mainnet keys are stored on offline hardware wallets.
- **Dependency auditing** — `cargo audit` runs in CI on every PR; all dependencies are pinned to exact versions and `Cargo.lock` is committed.
- **Responsible disclosure** — Report vulnerabilities privately via GitHub's security advisory feature. We follow coordinated disclosure with a 14-day fix window.

Full details: [`docs/governance-and-security.md`](docs/governance-and-security.md)

## Upgrading and Migration

When deploying a new contract version to production, follow the step-by-step
upgrade playbook which covers prerequisites, WASM upload, schema migration,
post-upgrade verification, and a full rollback procedure.

- **Migration guide** — schema versioning, data preservation, and example migration code: [`docs/migration-guide.md`](docs/migration-guide.md)
- **Upgrade playbook** — complete production runbook (pre-flight, upgrade, verify, rollback): [`docs/upgrade-playbook.md`](docs/upgrade-playbook.md)

## Coverage Thresholds

CI enforces minimum line-coverage percentages for the critical modules and
reports regressions relative to the `main` branch baseline.

| Module | Threshold |
|--------|-----------|
| `contract.rs` | ≥ 85 % |
| `rate_limiter.rs` | ≥ 90 % |
| `retry.rs` | ≥ 90 % |
| `transaction_state_tracker.rs` | ≥ 85 % |

Run locally:

```bash
./scripts/coverage_with_thresholds.sh

# Compare against a saved baseline and flag regressions > 2 %:
./scripts/coverage_with_thresholds.sh \
    --baseline /tmp/baseline.json \
    --report-delta
```

Full details: [`docs/coverage-metrics.md`](docs/coverage-metrics.md)

## Production Release Status

SorobanAnchor v0.1.0 is **production-ready**. The following verification and cleanup have been completed for this release:

### Build Matrix
- **Native (std):** `cargo build --release` — clean build, CLI binary produced
- **WASM (no_std):** `cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm` — clean build, optimized WASM produced
- **no_std verification:** `cargo check --no-default-features` passes

### Test Coverage
| Area | Status |
|------|--------|
| Feature gate combinations | All 8 documented combinations verified |
| Integration harness (local) | `cargo test --test cli_integration_harness` passes |
| Stress tests | `cargo test --features stress-tests` passes |
| Config validation | `./scripts/validate_all.sh` against all 6 example configs |
| Build matrix | `./scripts/test_build_matrix.sh` passes |

### Documentation & Cleanup
- All docs reviewed and updated for API accuracy
- Stale example scripts removed (`logging_demo.sh`, `anchor_routing_example.sh`)
- Error code references synced between `src/errors.rs`, `docs/error-codes.md`, and `docs/CONTRACT_FUNCTIONS.md`
- Build target docs aligned with actual `Makefile` targets
- Script path resolution bugs fixed in `pre_deploy_validate.sh`, `pre_deploy_validate.ps1`, `validate_all.sh`, `validate_all.ps1`
- Dead code removed from `ci_preflight_check.sh`, `verify_anchor_info_discovery.sh`
- `generate_hash_vectors.sh` fixed (function declaration ordering, reliable openssl fallback)
- Coverage script test file references corrected

### Release Artifacts
```bash
make release            # Build and bundle release artifacts
make release-validate   # Validate the release bundle
```
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
The release bundle includes:
- `anchorkit` — Native CLI binary
- `anchorkit.wasm` — Soroban WASM contract (optimized)
- `schemas/config_schema.json` — JSON schema
- `configs/` — Example anchor configurations
- `docs/` — Full documentation set
- `README.md`, `LICENSE`, `VERSION`

## License

MIT

## Handsoff notes

<!-- handsoff-issue-1105 -->
- #1105: 59. Bound maintenance-window scans

<!-- handsoff-issue-1106 -->
- #1106: 60. Reject zero streaming intervals

<!-- handsoff-issue-1107 -->
- #1107: 61. Prevent streaming cycle wrap
one negative test proving a zero or over-limit page size has explicit, bounded behavior, plus a regression test proving normal pages are untouched.

Everything in `<angle_brackets>` must be replaced with real names from the contract. Where the contract doesn't answer a question, this spec says to flag it rather than invent a limit.

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
