# Examples

This folder contains runnable examples demonstrating deployment, attestation, KYC, discovery, credentials, and offline flows. Use the task-oriented index below to find the example that fits your goal.

## Task-oriented index

Each entry lists the exact path, a one-line purpose, prerequisites, and whether it requires a live network or runs offline.

### Offline learning

Examples you can read and run without any network access or external services.

| Path | Purpose | Prerequisites | Network |
| --- | --- | --- | --- |
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

## Notes

- Paths listed in the index above are tracked and runnable; verify prerequisites before running any live-network example.
- For longer descriptions, refer to the examples table in the main README rather than duplicating it here.