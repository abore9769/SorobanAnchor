# Examples

This folder contains runnable examples demonstrating deployment, attestation, KYC, discovery, credentials, and offline flows. Use the task-oriented index below to find the example that fits your goal.

## Task-oriented index

Each entry lists the exact path, a one-line purpose, prerequisites, and whether it requires a live network or runs offline.

### Offline learning

Examples you can read and run without any network access or external services.

| Path | Purpose | Prerequisites | Network |
| --- | --- | --- | --- |
| `examples/minimal_library_usage.rs` | Smallest library example: imports the crate and normalizes a literal asset code with `anchorkit::normalize_asset_code`, handling the error case explicitly. Run with `cargo run --example minimal_library_usage`. | Rust toolchain (native target, default `std` feature) | Offline |
| `examples/invalid-config-sample.toml` | Intentionally-invalid configuration sample used to demonstrate config validation errors. | None | Offline |
| `examples/INVALID_CONFIG_SAMPLE.md` | Walkthrough of the intentionally-invalid config above and the expected validation failure. | None | Offline |

> `examples/invalid-config-sample.toml` and `examples/INVALID_CONFIG_SAMPLE.md` are a deliberate negative example. They are expected to fail validation; do not treat the sample as a working configuration.

### Operator workflow

Examples for day-to-day operator tasks such as deployment, attestation, and KYC.

| Path | Purpose | Prerequisites | Network |
| --- | --- | --- | --- |
| `examples/deploy_example.rs` | Demonstrates deploying the contract and running the deployment flow. | Rust toolchain; a configured network endpoint for live runs | Live network (deployment) |
| `examples/attestation_example.rs` | Demonstrates submitting and verifying attestations. | Rust toolchain; configured network endpoint and funded account for live runs | Live network (attestation) |
| `examples/kyc_example.rs` | Demonstrates the KYC verification flow. | Rust toolchain; configured network endpoint and test accounts for live runs | Live network (KYC) |

### Specialized protocol examples

Examples covering discovery, credentials, and offline flows.

| Path | Purpose | Prerequisites | Network |
| --- | --- | --- | --- |
| `examples/discovery_example.rs` | Demonstrates the discovery flow for locating and resolving records. | Rust toolchain; configured network endpoint for live runs | Live network (discovery) |
| `examples/credentials_example.rs` | Demonstrates issuing and verifying credentials. | Rust toolchain; configured network endpoint and test accounts for live runs | Live network (credentials) |
| `examples/offline_example.rs` | Demonstrates offline flows that do not require a live network. | Rust toolchain | Offline |

## Disabled examples

The following files are **disabled** and are **not runnable**. They are kept for reference only and are excluded from the index above:

- `examples/cli_example.rs.disabled`
- `examples/request_history_example.rs.disabled`

### Status

- **Not compiled.** Cargo only discovers `examples/*.rs` files, and neither file is listed as an `[[example]]` in `Cargo.toml`. `cargo build --examples` and CI never build them, so do not assume they compile against the current crate.
- **Not part of the supported API.** They call contract methods that do not exist in `src/contract.rs`:
  - `cli_example.rs.disabled` calls `set_supported_assets`, `is_asset_supported` and `update_health_status`, and imports `ServiceType` from the crate root, which does not re-export it.
  - `request_history_example.rs.disabled` calls `register_attestor_tracked`, `submit_attestation_tracked`, `get_request_history` and `get_api_call_details`.
- **Superseded by active examples with the same base name.** The active `examples/cli_example.rs` and `examples/request_history_example.rs` are the maintained versions. Changing or removing a `.disabled` file does not affect them.

### History

The git history records why they were disabled:

1. Commit `e78ac92c` ("fix: Disable broken example files") renamed both files to `.rs.disabled` because they "reference non-existent methods".
2. Commit `e5776892` ("feat: add stress tests and re-enable examples") added new `cli_example.rs` and `request_history_example.rs` files written against the current API. It left the `.disabled` copies in place.

The repository does not say why the `.disabled` copies were kept after the rewrite. Their retention is **historical/unclear**. Ask a maintainer before removing them.

### Evaluating whether to re-enable or remove one

1. Compare the disabled file with its active counterpart (`diff examples/cli_example.rs examples/cli_example.rs.disabled`). If the active example already covers the same workflow, the disabled copy has no unique value, which argues for removal.
2. If it shows something the active example does not, check every API it uses against the current exports in `src/lib.rs` and `src/contract.rs`. Do not add contract methods only to make an old example compile.
3. To re-enable it, port the unique part into the active example or into a new `examples/<name>.rs`. It must build with `cargo build --examples` and be added to the index above. Do not rename the `.disabled` file back while it still fails to compile.
4. Get maintainer agreement in the issue or PR before deleting either file.

## Notes

- Paths listed in the index above are tracked and runnable; verify prerequisites before running any live-network example.
- For longer descriptions, refer to the examples table in the main README rather than duplicating it here.