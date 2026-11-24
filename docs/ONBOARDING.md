# Maintainer Onboarding Guide

Welcome to the SorobanAnchor / AnchorKit project. This guide covers everything
a new maintainer needs to get oriented, set up locally, and start contributing
effectively without stepping on anything important.

---

## Table of Contents

1. [Repo overview](#1-repo-overview)
2. [Prerequisites](#2-prerequisites)
3. [Local setup](#3-local-setup)
4. [Key concepts](#4-key-concepts) (includes the [Glossary](#glossary))
5. [Day-to-day workflows](#5-day-to-day-workflows)
6. [Review and merge checklist](#6-review-and-merge-checklist)
7. [Release process](#7-release-process)
8. [Admin key access](#8-admin-key-access)
9. [Escalation and contacts](#9-escalation-and-contacts)

---

## 1  Repo overview

```
src/           Core library — contract, SEP handlers, utilities
tests/         Integration and unit tests
configs/       Example anchor configs (JSON + TOML)
examples/      Rust and shell usage examples
scripts/       Build, validation, CI, and deploy scripts
docs/          All project documentation (start here)
docs/adr/      Architecture Decision Records
benches/       Criterion benchmarks
fuzz/          Fuzz targets
```

The on-chain entry point is `src/contract.rs`. The public Rust API surface is
exposed through `src/lib.rs`. All stable error codes live in `src/errors.rs`
and are documented in [`docs/error-codes.md`](error-codes.md).

---

## 2  Prerequisites

Required for the documented build, test, and config-validation commands:

1. **Rust 1.75 or newer**, for `cargo build` and `make check`.
2. **The `wasm32-unknown-unknown` Rust target**, for the WASM build. Install it with `rustup target add wasm32-unknown-unknown`.
3. **Python 3 with the `jsonschema` package**, for `scripts/validate_config_strict.py`. On Python versions older than 3.11, also install `tomli` so TOML configs can be read.

Optional, needed only for the task named:

4. **`soroban-cli`**, to deploy and invoke contracts. See the commands in [`RUNBOOK.md`](RUNBOOK.md).
5. **Binaryen (`wasm-opt`)**, for WASM size optimization. The release packaging step runs it only when it is installed; see [Release Packaging](../README.md#release-packaging).
6. **GPG or minisign**, for signing release artifacts. See [`release-signing.md`](release-signing.md).
7. **Live testnet credentials**, for the opt-in smoke tests. See [`live-smoke-tests.md`](live-smoke-tests.md). Supply them as environment variables in your shell or your CI secret store. Do not write them into files, issues, or pull request comments.

---

## 3  Local setup

```bash
# Clone
git clone https://github.com/abore9769/SorobanAnchor.git
cd SorobanAnchor

# Install the pre-commit hook (runs fmt + clippy + test before each commit)
./scripts/setup-hooks.sh

# Verify everything builds and tests pass
make check
```

`make check` runs formatting, clippy, and the full test suite. It should be
green on a clean checkout. If it isn't, open an issue before doing anything
else.

### Environment variables

For testnet work:

```bash
export SOROBAN_NETWORK=testnet
export SOROBAN_RPC_URL=https://soroban-testnet.stellar.org:443
export SOROBAN_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
```

The live smoke tests also need `ANCHOR_CONTRACT_ID` and `ANCHOR_ADMIN_SECRET`.
Set them only in the shell session where you run those tests, or in CI secrets.
Do not add them to a config file or to any tracked file. See
[`live-smoke-tests.md`](live-smoke-tests.md) for the full list.

Never commit keys. The `.gitignore` already excludes `*.secret`, `*.key`,
`.env`, and `secrets/`.

---

### First-build troubleshooting

Use this table when the first build or test run fails. Each check is
read-only unless marked otherwise. For the full build options, see
[`docs/build-matrix.md`](build-matrix.md).

| Symptom | Check | Expected signal | Fix / reference |
|---------|-------|-----------------|-----------------|
| `cargo: command not found` | `cargo --version` | Prints a Cargo version | Install Rust with rustup (see [Prerequisites](#2--prerequisites)) |
| Rust is older than the minimum | `rustc --version` | Version below 1.75 | `rustup update stable` |
| WASM build fails with `can't find crate for \`core\`` | `rustup target list --installed` | `wasm32-unknown-unknown` is missing from the list | `rustup target add wasm32-unknown-unknown` |
| `cargo fmt` is missing | `rustup component list --installed` | `rustfmt` is missing from the list | `rustup component add rustfmt` |
| `no such command: \`clippy\`` | `rustup component list --installed` | `clippy` is missing from the list | `rustup component add clippy` |
| Formatting check fails | `cargo fmt --all -- --check` | Exits non-zero and prints a diff | Review the diff, then run `make fmt` (rewrites source files) |
| WASM build fails with `cannot find crate for \`clap\`` or `unresolved import \`std::fs\`` | `cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm` | Build succeeds with the WASM feature set | Drop the default `std` feature; see "Common Build Issues" in [`docs/build-matrix.md`](build-matrix.md) |
| Native build finishes but `target/release/anchorkit` is missing | `cargo build --release` | `Finished` line, and the binary exists under `target/release/` | Native builds need default features; see [`docs/build-matrix.md`](build-matrix.md) |
| Focused mock test runs 0 tests | `cargo test --features mock-only test_sep6_deposit_minimal_fields` | `test result: ok. 1 passed` | Keep `--features mock-only`; the test is not compiled without it |
| Full local suite fails on a clean checkout | `cargo test` | All tests pass | Run `git status` first; if the tree is clean and the failure persists, open an issue |
| Live integration target fails immediately | `make integration-test` (local, no network) | Passes | `make integration-test-live` needs `ANCHOR_CONTRACT_ID` and `ANCHOR_ADMIN_SECRET`; see the README's "Live testnet tests" |

If a check fails on a clean checkout with the toolchain above in place, treat
it as a real source failure and open an issue with the command and full output.

### Test layers

The repository has several test layers. Pick the cheapest one that covers your
change.

| Layer | Purpose | Prerequisites | Command | Cost |
|-------|---------|---------------|---------|------|
| Fast local check | One focused mock-only test | Rust toolchain | `cargo test --features mock-only test_sep6_deposit_minimal_fields` | Seconds to build and run |
| Complete local suite | All unit and integration tests, with mock fixtures | Rust toolchain | `cargo test --features std,mock-only` | Full compile; no network |
| Integration harness | Deploy → register → attest → verify in the local Soroban simulation | Rust toolchain | `make integration-test` | Local only; no network |
| Extended pipeline | Admin transfer, capabilities, batching, routing fallback, and related flows | Rust toolchain | `make integration-test-pipeline` | Local only; no network |
| Property-based tests | Randomized inputs for parsers and validators (`proptest`, in `tests/property_based_parsing_tests.rs`) | Rust toolchain with `std` (default) | Included in `cargo test` | Local only; no network |
| Live testnet checks | Confirm a deployed contract on testnet is reachable and correct | Network access, `SOROBAN_ANCHOR_INTEGRATION=testnet`, `ANCHOR_CONTRACT_ID`, `ANCHOR_ADMIN_SECRET` | `make integration-test-live` or `make smoke-test-live` | Opt-in; depends on the network; never run in normal CI |
| Stress tests | High-concurrency and burst-load scenarios (`tests/load_simulation_tests.rs`) | Rust toolchain | `make stress-test` | Slow; excluded from normal CI |
| Fuzz targets | Malformed-input robustness for parsers (see [`fuzz/README.md`](../fuzz/README.md)) | `cargo install cargo-fuzz` | `cd fuzz && cargo fuzz run fuzz_target_json_response_parsing -- -max_len=1024 -runs=100` | Smoke run is quick; without `-runs` it runs until interrupted |

Live checks use real network calls and secrets, so run them only when your
change touches on-chain behavior and you have credentials. Keep secrets out of
shell history.

To review a regression snapshot change, see
[`docs/snapshot-testing.md`](snapshot-testing.md).

## 4  Key concepts

### Build modes

The project compiles in two distinct modes with completely separate feature sets:

| Mode | Command | Output | Has CLI |
|------|---------|--------|---------|
| Native | `cargo build --release` | `target/release/anchorkit` | Yes |
| WASM (Soroban) | `cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm` | `*.wasm` | No |

Always test both before touching anything in `src/contract.rs` or `src/lib.rs`.

### Feature flags

| Flag | Purpose |
|------|---------|
| `std` | Filesystem config loading (default on) |
| `wasm` | Soroban WASM target — strips HTTP/CLI modules |
| `mock-only` | Pre-built fixtures for tests without a live anchor |
| `stress-tests` | High-concurrency load tests (excluded from normal CI) |

### Error codes

All contract errors are stable integer codes defined in `src/errors.rs` and
listed in [`docs/error-codes.md`](error-codes.md). Do not renumber existing
codes — add new ones at the end only.

### Schema versioning

The on-chain data schema is versioned. Every data-shape change needs a
migration step in `src/migration.rs`. See
[`docs/migration-guide.md`](migration-guide.md) for the full process.

### Admin model

The contract has a primary admin (set at `initialize`) plus a capability
delegation system. Capability codes and grant/revoke procedures are documented
in [`docs/RUNBOOK.md`](RUNBOOK.md#admin-capability-management).

---

### Glossary

Project terms that carry a protocol-specific meaning. Each entry points to the
canonical document for details.

| Term | Meaning | Canonical reference |
|------|---------|---------------------|
| **Anchor** | The off-chain service that SEP-6 and SEP-24 deposit and withdrawal flows run against. Each anchor has an example config under `configs/`. | [ARCHITECTURE.md](ARCHITECTURE.md) |
| **Attestor** | An address registered with `register_attestor` that may submit attestations. | [CONTRACT_FUNCTIONS.md](CONTRACT_FUNCTIONS.md#attestor-management), [attestor-roles.md](attestor-roles.md) |
| **Issuer** | In contract calls, the `issuer` argument of `submit_attestation`. `sep10_issuer` on `register_attestor` names the SEP-10 issuer for an attestor's tokens. | [CONTRACT_FUNCTIONS.md](CONTRACT_FUNCTIONS.md#attestations) |
| **Contract (on-chain)** | The Soroban contract built from `src/contract.rs`. Its entry points are the functions documented in the contract reference. | [CONTRACT_FUNCTIONS.md](CONTRACT_FUNCTIONS.md) |
| **Host runtime** | The native (`std`) side of the project: the CLI, config loading, response validation, and test harnesses that run outside the on-chain contract. | [ARCHITECTURE.md](ARCHITECTURE.md#native-vs-wasm-split) |
| **WASM artifact** | The `anchorkit.wasm` file produced by the WASM build (`make wasm`) at `target/wasm32-unknown-unknown/release/`. | [BUILD_TARGETS.md](BUILD_TARGETS.md) |
| **Schema version** | The version of the on-chain data layout. Read with `get_schema_version` and advanced by `migrate`. | [migration-guide.md](migration-guide.md) |
| **Primary admin** | The address set by `initialize`. It holds every role and capability implicitly. | [CONTRACT_FUNCTIONS.md](CONTRACT_FUNCTIONS.md#authorization-model) |
| **Role** | A scoped set of admin operations granted with `grant_role`, such as `KycAdmin` or `AttestorAdmin`. | [CONTRACT_FUNCTIONS.md](CONTRACT_FUNCTIONS.md#authorization-model) |
| **Capability** | A single fine-grained admin operation granted with `grant_capability`, such as `ManageAttestors`. | [CONTRACT_FUNCTIONS.md](CONTRACT_FUNCTIONS.md#authorization-model) |
| **SEP-10 JWT** | The EdDSA-signed token that the contract verifies using stored Ed25519 public keys. | [README.md](../README.md#what-it-does) |

---

## 5  Day-to-day workflows

### Network profile precedence

Custom network profiles are stored in `~/.anchorkit/networks.json`. Each
profile has a `name`, an `rpc_url`, a `network_passphrase`, an optional
`horizon_url`, and an `is_default` flag. Profiles hold endpoint metadata only.
They never store signing keys. Signing credentials are supplied separately, as
described in the signing key section below.

Which network a command uses depends on whether the subcommand has its own
`--network` option.

**Commands without their own `--network` option** (for example `doctor`) use
this order, highest first:

1. The global `--network <name>` flag.
2. The `STELLAR_NETWORK` environment variable.
3. The profile with `is_default: true`, set with `anchorkit network set-default --name <NAME>`.
4. `testnet`.

**Commands with their own `--network` option** (`deploy`, `register`,
`attest`, `quote`, `revoke`, `health`, `verify`) use only that option's value.
The option defaults to `testnet`. These commands do not read `STELLAR_NETWORK`
or the default profile. Pass `--network` explicitly on these commands so the
target is never set by an inherited environment variable.

Once a name is chosen, its endpoints are resolved as follows:

- A custom profile with that name supplies `rpc_url` and `network_passphrase`.
- Otherwise a built-in name (`testnet`, `mainnet`, `futurenet`) uses the built-in endpoints.
- Otherwise the name is unknown. Commands that read the global network fall back to the testnet endpoint with a warning. Commands with their own `--network` option reject unknown names before doing any work.

Example, using placeholder values only:

```bash
# Register a custom profile (placeholder URL and passphrase)
anchorkit network add --name my-net \
  --rpc-url https://rpc.example.invalid \
  --passphrase "Example Passphrase"

# Make it the default for commands that use the global network
anchorkit network set-default --name my-net

export STELLAR_NETWORK=my-net
anchorkit doctor                     # uses my-net

# Pass --network explicitly for deploy-style commands
anchorkit deploy --network my-net
```

### CLI flag and environment variable precedence

Several CLI flags read from environment variables when the flag is omitted.
When both are supplied, the explicit flag always wins.

| Flag | Environment variable | Notes |
|---|---|---|
| `--contract-id` | `ANCHOR_CONTRACT_ID` | Contract to operate on; required for most subcommands |
| `--network` | `STELLAR_NETWORK` | `testnet`, `mainnet`, `futurenet`, or a custom profile name |
| `--no-interactive` | `ANCHORKIT_NO_INTERACTIVE` | Set to `1` to suppress all interactive prompts |
| `--ephemeral-token` | `ANCHORKIT_EPHEMERAL_TOKEN` | Single-use signing token; highest priority over all other key sources |

For signing key resolution the precedence order is, from highest to lowest:

1. `--ephemeral-token` / `ANCHORKIT_EPHEMERAL_TOKEN`
2. `--secret-key`
3. `ANCHOR_ADMIN_SECRET` environment variable
4. `--keypair-file`
5. `--credential-name` (interactive keystore; not available when `--no-interactive` is set)

Example — flag overrides the environment variable:

```bash
export STELLAR_NETWORK=testnet
anchorkit deploy --network mainnet   # deploys to mainnet despite the env var
```

Never put real keys in shell history. Prefer `ANCHOR_ADMIN_SECRET` for
automated flows, or `--keypair-file` pointing to a file excluded from version
control.

### Handling CLI secrets safely

Shell history and process listings can expose a secret that is typed on the
command line. Anything typed as an argument can be saved in `~/.bash_history`
or a similar file, and it can be visible to other users through `ps` while the
command runs. Log files and terminal recordings can capture the same values.

Unsafe: the secret is an argument, so it lands in shell history and the
process list.

```bash
# UNSAFE — placeholder value shown; never type a real key as an argument
anchorkit credentials add --name anchor-admin --value "S-PLACEHOLDER-NOT-A-REAL-KEY"
```

Safer: omit `--value`. The command then prompts for the secret, and for the
keystore password and its confirmation, using hidden terminal input.

```bash
# SAFE — the secret and passwords are entered at hidden prompts
anchorkit credentials add --name anchor-admin
```

Other points:

- The keystore is stored at `~/.anchorkit/credentials.json`, and each entry is encrypted with the keystore password. `credentials add`, `credentials get` and `credentials rotate` need an interactive terminal. They fail when `--no-interactive` or `ANCHORKIT_NO_INTERACTIVE` is set.
- `credentials get --name <NAME>` prints the decrypted secret to standard output. Do not run it where output is logged, recorded or piped to a file, and do not paste its output into tickets or chat.
- For automated flows, load `ANCHOR_ADMIN_SECRET` from a secrets manager or a protected file rather than typing it, for example `export ANCHOR_ADMIN_SECRET="$(cat /path/to/protected/file)"`. The value still appears in the environment of the process, so restrict who can read that file.
- Keep any key file used with `--keypair-file` out of version control, and encrypt it at rest.
- Proxy credentials are configured separately. See [`proxy-and-credentials.md`](proxy-and-credentials.md) for how they are set and handled.
- For encrypting files that hold secrets at rest, see [`secret-file-encryption.md`](secret-file-encryption.md).
- For a table of every `anchorkit credentials` operation, including which ones are destructive, see [`credential-lifecycle.md`](credential-lifecycle.md).

### Making a change

```bash
git checkout -b feat/short-description
# ... edit ...
make check          # must be green before committing
git add <files>
git commit -m "feat(scope): description"
git push -u origin feat/short-description
# open a PR
```

Follow [conventional commits](https://www.conventionalcommits.org/):
`feat`, `fix`, `docs`, `refactor`, `test`, `chore`.

### First contribution task

A safe first contribution is a focused mock-only parser check that does not
require network credentials, live services, or deployment access. Start with the
existing SEP-6 fixture in [`tests/sep_fixtures_tests.rs`](../tests/sep_fixtures_tests.rs):

```bash
cargo test --features mock-only test_sep6_deposit_minimal_fields
```

This validates the current mock fixture path in [`src/mock.rs`](../src/mock.rs)
and confirms the parser accepts a minimal synthetic deposit response. If the
command passes, you have a working local proof and can move on to a small
follow-up change or documentation fix before opening a PR.

### Running tests

```bash
cargo test                              # standard suite
cargo test --features mock-only         # with mock fixtures
cargo test --test cli_integration_harness  # integration harness
cargo test --features stress-tests      # load tests (slow)
```

### Config validation

```bash
./scripts/validate_all.sh       # validates all configs in configs/
./scripts/pre_deploy_validate.sh  # full pre-deploy check
```

**Strict unknown-field rejection.** The schema uses `additionalProperties: false`
on every object section. Any key not listed in the schema causes validation to
fail immediately — this catches typos before they reach a running anchor. For
example, writing `monitorring` instead of `monitoring` produces:

```
schema validation failed:
(root): [additionalProperties] Additional properties are not allowed ('monitorring' was unexpected)
```

Correct the spelling and re-run validation. If you need values that vary per
environment (feature flags, credentials, local overrides) keep them out of the
anchor config file entirely and pass them via environment variables or a
separate secrets store. The anchor config is for structural deployment
configuration only.

### Deliberately invalid sample

[`examples/invalid-config-sample.toml`](../examples/invalid-config-sample.toml)
is a non-deployable sample with one intentional schema error: its synthetic
attestor address does not match the required Stellar address format. All other
required fields are present so the validator reports that specific problem.
Run the offline CLI from the repository root; a validation failure is expected:

```bash
cargo run -- offline validate --config examples/invalid-config-sample.toml
```

The validator should report an address-format/schema validation failure. This
command only reads local files and does not contact the network.

### Generating API docs

Refresh the browsable crate reference whenever the public exports change:

```bash
make api-docs
```

The generated HTML lands in `target/api-docs/doc/anchorkit/index.html` and is
built directly from the crate's public modules and re-exports in
`src/lib.rs`, so it stays aligned with the current public API surface.

### API contract snapshots

To capture and compare the public API surface across versions, see
[`docs/api-contract-snapshots.md`](api-contract-snapshots.md).

### Generating a changelog

Changelog entries are generated from conventional commit history. See
[`docs/changelog-generation.md`](changelog-generation.md).

---

## 6  Review and merge checklist

Before merging any PR:

- [ ] `make check` passes (fmt + clippy + tests)
- [ ] Both native and WASM builds succeed
- [ ] New public API items have doc comments
- [ ] Error codes are not renumbered
- [ ] Config schema updated if new fields added
- [ ] Migration step added for any on-chain data shape change
- [ ] Security review completed for auth / crypto / HTTP changes
  (see [`docs/SECURITY_REVIEW_CHECKLIST.md`](SECURITY_REVIEW_CHECKLIST.md))

Routine changes need **one** maintainer approval. Breaking changes, contract
upgrades, or security model changes need **two** approvals and a 48-hour window.
Full policy: [`docs/governance-and-security.md`](governance-and-security.md).

---

## 7  Release process

```bash
make release           # build native + WASM + bundle under dist/
make release-validate  # verify bundle contents and checksums
make release-sign      # sign artifacts (requires GPG key)
```

Tag the release:

```bash
git tag -s v0.x.0 -m "Release v0.x.0"
git push origin v0.x.0
```

Full release runbook: [`docs/RUNBOOK.md`](RUNBOOK.md).
Signing and verification: [`docs/release-signing.md`](release-signing.md).

---

## 8  Admin key access

- Testnet keys: ask an existing maintainer; rotated after each major release.
- Mainnet keys: stored on offline hardware wallets with 2-of-N multi-sig.
  No single person holds all signing keys.
- Key operations follow the same two-maintainer approval process as contract
  upgrades.

If you need mainnet key access, open a governance discussion in the repository
before any key-sharing happens.

---

## 9  Escalation and contacts

- **Bugs / feature requests** — open a GitHub issue.
- **Security vulnerabilities** — do not open a public issue; use GitHub's
  private advisory reporting at
  `https://github.com/abore9769/SorobanAnchor/security/advisories/new`.
  See [`docs/governance-and-security.md`](governance-and-security.md#responsible-disclosure).
- **PR review requests** — tag `@maintainers` in the PR.
- **Urgent production issues** — contact the on-call maintainer directly via
  the contact method shared in the private maintainer channel.

---

## References

- [README.md](../README.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [RUNBOOK.md](RUNBOOK.md)
- [Architecture Decision Records](adr/)
- [governance-and-security.md](governance-and-security.md)
- [error-codes.md](error-codes.md)
- [migration-guide.md](migration-guide.md)
- [api-contract-snapshots.md](api-contract-snapshots.md)
- [changelog-generation.md](changelog-generation.md)
