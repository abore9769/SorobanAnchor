# Contributing to AnchorKit

Thank you for contributing to AnchorKit! This guide explains our coding standards and how to ensure your code meets project requirements before submitting a pull request.

## Code Quality Standards

AnchorKit enforces consistent code formatting and quality through automated tools:

- **rustfmt** — Automatic code formatting (configured in `rustfmt.toml`)
- **clippy** — Linting and best practices (configured in `.clippy.toml`)
- **cargo test** — Unit and integration tests

## Before Opening a PR

Copy this block to run the minimum local checks. Every command exists in this
repository. Commands marked *optional* are not required for a PR.

```bash
make fmt-check                          # rustfmt check, no files changed
make lint                               # cargo clippy -- -D warnings
make test                               # cargo test (default features)
cargo check --no-default-features       # no_std compile check for the core library
make wasm                               # WASM build (wasm32-unknown-unknown, --features wasm)
make doc-lint                           # documentation link and command checks (may install markdownlint-cli via npm)

# Optional: feature-gated mock fixtures (see docs/build-matrix.md)
cargo test --features mock-only

# Optional: live testnet tests. These need network access and
# ANCHOR_CONTRACT_ID and ANCHOR_ADMIN_SECRET; they are not required for a PR.
make smoke-test-live
```

## Pre-Commit Validation

Before committing or pushing code, run the complete validation suite:

```bash
make check
```

This runs:
1. `cargo fmt --all -- --check` — Verify formatting
2. `cargo clippy --all-targets --all-features -- -D warnings` — Lint checks
3. `cargo test` — All tests

### Individual Checks

If you prefer to run checks individually:

```bash
# Check formatting (without modifying files)
make fmt-check

# Auto-fix formatting issues
make fmt

# Run linting checks
make lint

# Run tests
make test

# Build release binary
make build

# Build WASM target
make wasm

# Verify no-default-features compatibility (no_std check)
cargo check --no-default-features
```

### Feature Compatibility Check

The project supports both native (std) and WASM (no_std) builds with distinct feature sets. To verify that the library compiles without default features (no_std compatibility):

```bash
cargo check --no-default-features
```

This check ensures that the core library remains compatible with no_std environments. It does **not** produce a WASM artifact — for that, use the full WASM build command:

```bash
cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm
```

**When to run:**
- Before committing changes to core library code
- After modifying feature gates or conditional compilation
- When adding new dependencies

**What it validates:**
- Core library compiles without std feature
- No accidental dependencies on std-only code
- Feature gates are correctly configured

**Note:** This is a compile check only. The native CLI (`main.rs`) and host-only modules are excluded when std is disabled, which is expected and correct.

## Formatting Rules

AnchorKit uses `rustfmt` with the following key rules (see `rustfmt.toml` for full config):

- **Line length:** 100 characters max
- **Indentation:** 4 spaces (no tabs)
- **Imports:** Grouped and reordered (std, external crates, internal)
- **Trailing commas:** Vertical style
- **Comments:** Wrapped at 80 characters

### Auto-formatting

To automatically fix formatting issues:

```bash
cargo fmt --all
```

Or use the Makefile:

```bash
make fmt
```

## Linting Standards

AnchorKit uses `clippy` with strict warnings-as-errors policy. All clippy warnings must be resolved before merging.

Key linting rules:

- **Cognitive complexity:** Max 30 (default: 25)
- **Type complexity:** Max 500 (default: 250)
- **Function arguments:** Max 8 (default: 7)
- **Unsafe blocks:** Discouraged; must be justified with comments
- **Documentation:** Public items should have doc comments

### Common Clippy Warnings

| Warning | Fix |
|---------|-----|
| `clippy::too_many_arguments` | Refactor function to use a struct parameter |
| `clippy::cognitive_complexity` | Break function into smaller helpers |
| `clippy::type_complexity` | Use type aliases or extract to a struct |
| `clippy::needless_borrow` | Remove unnecessary `&` or `&mut` |
| `clippy::match_like_matches_macro` | Use `matches!()` macro |

Run linting checks:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Or use the Makefile:

```bash
make lint
```

## Documentation Validation

After editing documentation files, validate them with the docs validation script:

```bash
bash scripts/validate-docs.sh
```

This script checks:
- **Markdown lint** — heading hierarchy, code blocks, formatting via markdownlint-cli
- **Broken internal links** — relative links between docs
- **Heading consistency** — duplicate or malformed top-level headings
- **Command example hygiene** — bare `cargo`, `make`, and shell commands
- **Stale placeholder text** — TODO / FIXME / TBD / `<YOUR_` patterns

The script runs from the repository root and validates all files in `docs/` plus `README.md`. To auto-fix markdownlint issues where possible:

```bash
bash scripts/validate-docs.sh --fix
```

To check a single file:

```bash
bash scripts/validate-docs.sh --file docs/YOUR_DOC.md
```

This same validation runs in CI under the "Documentation Lint" job in `.github/workflows/code-quality.yml`.

## Documentation Standards

All public functions and types should have doc comments:

```rust
/// Brief description of what this function does.
///
/// More detailed explanation if needed. Explain the purpose, behavior,
/// and any important side effects.
///
/// # Arguments
///
/// * `param1` - Description of param1
/// * `param2` - Description of param2
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Panics or returns errors with:
/// - `ErrorCode::SomeError` - when this condition occurs
///
/// # Examples
///
/// ```rust
/// let result = my_function(arg1, arg2);
/// assert_eq!(result, expected);
/// ```
pub fn my_function(param1: Type1, param2: Type2) -> ReturnType {
    // implementation
}
```

## Public API change review

AnchorKit exposes public Rust types and functions, and API snapshots record
changes to those exports. Before changing anything that is publicly exported,
review each of the following surfaces:

- **Public exports** — items exported from `src/lib.rs` (and the modules it
  re-exports). Check whether a change adds, removes, or alters a public type,
  function, trait, or signature.
- **API snapshots** — snapshot files under `api_snapshots/`, produced by
  `scripts/snapshot_api.sh`. Regenerate the snapshot when exports change and
  include the result in your PR.
- **Snapshot diff** — compare the snapshot against the previous one with
  `scripts/diff_api_snapshot.sh`. The procedure is documented in
  [api-contract-snapshots.md](api-contract-snapshots.md); follow it and report
  the diff in your PR description.
- **Documentation** — update doc comments and the pages listed in the
  [Documentation Ownership Map](#documentation-ownership-map) when behavior or
  usage changes.
- **Tests** — update or add tests that exercise the changed public surface.

Note: the repository does not currently promise semantic-versioning stability
for its public API. Treat the snapshot diff as a record of what changed, not as
a guarantee that existing exports will remain compatible across releases. If a
semver policy is adopted later, this note will be updated to reference it.

## Generated files

Some files in the repository are produced by scripts, while others are written by hand. Check which kind you are looking at before editing.

| Output | Generated by | Committed? | Edit by hand? |
|--------|--------------|------------|---------------|
| `target/api-docs/` (HTML API reference) | `make api-docs` (`scripts/generate_api_docs.sh`) | Ignored (`/target` in `.gitignore`) | No |
| `dist/` (release bundle, tarball, `.sha256`) | `make release` (`scripts/package_release.sh`) | Ignored (`/dist`, `*.tar.gz`, `*.sha256`) | No |
| `CHECKSUMS.sha256` inside the bundle | `scripts/package_release.sh` | Inside the ignored `dist/` tree | No |
| `api_snapshots/anchorkit-<VERSION>.json` | `scripts/snapshot_api.sh` | Not ignored by `.gitignore`; only `api_snapshots/.gitkeep` is tracked today | No |
| `test_snapshots/` fixtures | Committed fixtures; hash vectors can be re-printed with `scripts/generate_hash_vectors.sh` | Yes (tracked) | Only deliberately, with the test that reads them |
| `Cargo.lock` | Cargo | Ignored (`Cargo.lock` in `.gitignore`) | No |

Notes:

- The release bundle layout and `CHECKSUMS.sha256` contents are defined in `scripts/package_release.sh`. See [release-signing.md](release-signing.md) for signing and verification.
- `scripts/release-checklist.sh` compares the API snapshot with `scripts/diff_api_snapshot.sh`. See [api-contract-snapshots.md](api-contract-snapshots.md) for the snapshot workflow.
- `scripts/generate_hash_vectors.sh` prints test vectors for you to paste into `tests/cross_platform_tests.rs`; it does not update the fixtures by itself.

## Roadmap and milestones

The project roadmap is maintained in [ROADMAP.md](ROADMAP.md). It describes current milestone phases, planned work, and how to link issues to milestones.

## Issue triage

New issues are triaged and prioritized according to the process in [TRIAGE.md](TRIAGE.md). If you are picking up an issue, check the priority labels and milestone assignment there.

## Branch and PR hygiene

Branch naming conventions, PR workflow, and local automation are documented in [branch-pr-hygiene.md](branch-pr-hygiene.md). Run the branch name validator before pushing:

```bash
./scripts/validate-branch-name.sh
```

## Architecture Decision Records

Important architectural decisions are captured as Architecture Decision Records
(ADRs) in [`docs/adr/`](adr/). Before making a significant design change, review
the existing ADRs and consider whether a new ADR is needed.

- [ADR-0001](adr/ADR-0001-record-architecture-decisions.md): Record Architecture Decisions
- [ADR-0002](adr/ADR-0002-dual-build-strategy.md): Dual-Build Strategy
- [ADR-0003](adr/ADR-0003-contract-lifecycle-and-schema-versioning.md): Contract Lifecycle and Schema Versioning

## Documentation Ownership Map

When a change affects behavior, check the pages below before opening a PR. Each
page is the owner of its topic. Link to it rather than repeating its steps.

| If your change touches... | Review these pages |
|---------------------------|--------------------|
| Protocol behavior or contract functions | [CONTRACT_FUNCTIONS.md](CONTRACT_FUNCTIONS.md), [error-codes.md](error-codes.md) |
| Configuration and config validation | [RUNBOOK.md](RUNBOOK.md) (Configuration Validation), [attestor-roles.md](attestor-roles.md), [`configs/`](../configs/) |
| Contract upgrades and schema changes | [upgrade-playbook.md](upgrade-playbook.md), [migration-guide.md](migration-guide.md) |
| Release process | [release-checklist.md](release-checklist.md), [release-signing.md](release-signing.md), [changelog-generation.md](changelog-generation.md) |
| Security-sensitive code (auth, crypto, HTTP, deployment) | [SECURITY_REVIEW_CHECKLIST.md](SECURITY_REVIEW_CHECKLIST.md), [governance-and-security.md](governance-and-security.md) |
| Service operation | [service-management.md](service-management.md), [RUNBOOK.md](RUNBOOK.md) |
| Build targets and feature flags | [build-matrix.md](build-matrix.md), [BUILD_TARGETS.md](BUILD_TARGETS.md) |

## Git Workflow

1. **Create a feature branch:**
   ```bash
   git checkout -b feature/description-of-change
   ```

2. **Make your changes** and commit regularly:
   ```bash
   git add .
   git commit -m "feat: description of change"
   ```

3. **Run validation before pushing:**
   ```bash
   make check
   ```

4. **Push to remote:**
   ```bash
   git push -u origin feature/description-of-change
   ```

5. **Create a pull request** with a clear description

## Commit Message Format

Follow conventional commits:

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`

Example:
```
feat(contract): add new attestation verification

Implement Ed25519 signature verification for attestations
with replay attack detection.

Closes #123
```

## Testing

All new features should include tests:

```bash
cargo test
```

Run tests with output:

```bash
cargo test -- --nocapture
```

Run a specific test:

```bash
cargo test test_name
```

## Release checklist

Before tagging a release, run the automated checklist:

```bash
./scripts/release-checklist.sh <version>
```

Full documentation is in [release-checklist.md](release-checklist.md).

## CI/CD Pipeline

The project uses GitHub Actions for continuous integration. All checks must pass before merging:

- ✓ Formatting check (`cargo fmt -- --check`)
- ✓ Linting (`cargo clippy -- -D warnings`)
- ✓ Tests (`cargo test`)
- ✓ WASM build (`cargo build --target wasm32-unknown-unknown`)
- ✓ Dependency audit (`dependency-audit.yml`)
- ✓ Feature matrix (`feature-matrix.yml` — all feature-flag combinations)

## Security Review

Changes that touch authentication, cryptography, authorization, HTTP
integrations, or the deployment process require a security review in addition
to the standard code review. Use the repository's
[Security Review Checklist](SECURITY_REVIEW_CHECKLIST.md) as a guide.

See [Governance and Security](governance-and-security.md) for the full policy,
including which changes require two maintainer approvals.

## Troubleshooting

### Formatting conflicts

If `cargo fmt` and `clippy` disagree, `rustfmt` takes precedence. Run `cargo fmt` first, then `cargo clippy`.

### Clippy false positives

If you believe a clippy warning is a false positive, you can suppress it with:

```rust
#[allow(clippy::lint_name)]
fn my_function() {
    // implementation
}
```

Always add a comment explaining why the lint is suppressed.

### Performance issues

If `cargo clippy` is slow, you can run it with fewer features:

```bash
cargo clippy --lib -- -D warnings
```

## Questions?

- Check existing issues and PRs
- Review the project README and documentation
- Ask in PR comments or discussions

Thank you for helping make AnchorKit better!
