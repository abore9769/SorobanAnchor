# Contract Deployment

This guide covers building, deploying, and initialising the `AnchorKitContract`
on the Stellar network using the Stellar CLI and the built-in `anchorkit` CLI
binary.

## Prerequisites

- Rust with `wasm32-unknown-unknown` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli) (`stellar`)
- A funded Stellar account (source keypair)

## 1. Build the WASM

```bash
cd SorobanAnchor
cargo build --release --target wasm32-unknown-unknown
```

The optimised WASM is at:
`target/wasm32-unknown-unknown/release/anchorkit.wasm`

## 2. Deploy with the Stellar CLI

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/anchorkit.wasm \
  --source <YOUR_SECRET_KEY> \
  --network testnet
```

The command prints the contract ID. Save it for the next step.

## 3. Deploy with the Built-in CLI

AnchorKit ships a `deploy` subcommand that wraps the Stellar CLI:

```bash
cargo run --bin anchorkit -- deploy \
  --network testnet \
  --source-key <YOUR_SECRET_KEY>
```

Environment variable alternative:

```bash
export STELLAR_SECRET_KEY=<YOUR_SECRET_KEY>
cargo run --bin anchorkit -- deploy --network testnet
```

## 4. Initialise the Contract

After deployment, call `initialize` to set the admin address:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_SECRET_KEY> \
  --network testnet \
  -- initialize \
  --admin <ADMIN_ADDRESS>
```

## 5. Register an Attestor

```bash
cargo run --bin anchorkit -- register \
  --contract <CONTRACT_ID> \
  --attestor <ATTESTOR_ADDRESS> \
  --sep10-token <JWT_TOKEN> \
  --sep10-issuer <ISSUER_ADDRESS> \
  --network testnet \
  --source-key <ADMIN_SECRET_KEY>
```

## 6. Submit an Attestation

```bash
cargo run --bin anchorkit -- attest \
  --contract <CONTRACT_ID> \
  --subject <SUBJECT_ADDRESS> \
  --data "kyc_approved" \
  --network testnet \
  --source-key <ATTESTOR_SECRET_KEY>
```

## 7. Check Environment Setup

The `doctor` subcommand verifies that all required tools are installed:

```bash
cargo run --bin anchorkit -- doctor
```

## 8. Revoke an Attestor

```bash
cargo run --bin anchorkit -- revoke \
  --contract <CONTRACT_ID> \
  --attestor <ATTESTOR_ADDRESS> \
  --network testnet \
  --source-key <ADMIN_SECRET_KEY>
```

## Build Profiles

| Profile | Description |
|---------|-------------|
| `release` | Optimised for size (`opt-level="z"`), LTO, symbols stripped, `panic=abort` |
| `release-with-logs` | Same as `release` but with `debug-assertions = true` for logging |

## Feature Flags

| Flag | Description |
|------|-------------|
| `std` (default) | Standard library support for off-chain use |
| `wasm` | Target Soroban WASM environment |
| `mock-only` | Compile only mock/test helpers |
| `stress-tests` | Enable load-simulation test suite |

Build for WASM without std:

```bash
cargo build --release --target wasm32-unknown-unknown \
  --no-default-features --features wasm
```

## Contract Storage TTLs

| Storage type | TTL (ledgers) | ~Duration |
|--------------|---------------|-----------|
| Persistent (attestors, attestations, KYC) | 1,555,200 | ~90 days |
| Instance (admin, config) | 518,400 | ~30 days |
| Temporary (tracing spans) | 17,280 | ~1 day |
| Transaction state | 1,555,200 | ~90 days |

TTLs are automatically extended on each access.

## Generating API Documentation

```bash
cargo doc --no-deps --open
```

This produces a complete, navigable documentation site in `target/doc/`.
