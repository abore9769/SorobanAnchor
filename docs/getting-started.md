# Getting Started with AnchorKit

AnchorKit (`anchorkit`) is a Soroban smart-contract library for building and
interacting with Stellar anchor services. It ships both an on-chain contract
layer and an off-chain service layer that normalises responses from anchors
implementing the [Stellar Ecosystem Proposals (SEPs)](https://github.com/stellar/stellar-protocol/tree/master/ecosystem).

## Prerequisites

- Rust 1.74+ with the `wasm32-unknown-unknown` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli) for contract deployment
- A Stellar testnet or mainnet account with XLM for fees

Install the WASM target:

```bash
rustup target add wasm32-unknown-unknown
```

## Adding AnchorKit to Your Project

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
anchorkit = { path = "../SorobanAnchor" }   # local
# or from crates.io once published:
# anchorkit = "0.1"
```

## Library Structure

```
anchorkit
├── contract        – On-chain Soroban contract (AnchorKitContract)
├── sep6            – SEP-6 non-interactive deposit/withdrawal
├── sep24           – SEP-24 interactive deposit/withdrawal
├── sep38 (pub re-export) – SEP-38 anchor RFQ / firm quotes
├── errors          – AnchorKitError + ErrorCode
├── domain_validator – HTTPS URL validation
├── rate_limiter    – Per-attestor sliding-window rate limiting
├── retry           – Exponential-backoff retry
├── sep10_jwt       – EdDSA JWT verification (SEP-10)
├── deterministic_hash – Canonical SHA-256 hashing
├── transaction_state_tracker – On-chain state machine
└── response_validator – Anchor API response schema validation
```

## Quick Example

```rust
use anchorkit::{
    validate_anchor_domain,
    sep6::{initiate_deposit, RawDepositResponse},
    retry::{retry_with_backoff, RetryConfig},
};

fn main() {
    // 1. Validate the anchor domain before any outbound request.
    validate_anchor_domain("https://anchor.example.com")
        .expect("invalid anchor domain");

    // 2. Normalise a SEP-6 deposit response.
    let raw = RawDepositResponse {
        transaction_id: "txn-001".into(),
        how: "Send to bank account 1234".into(),
        extra_info: None,
        min_amount: Some(10),
        max_amount: Some(10_000),
        fee_fixed: Some(1),
        status: Some("pending_external".into()),
        clawback_enabled: None,
        stellar_memo: None,
        stellar_memo_type: None,
    };
    let deposit = initiate_deposit(raw).expect("invalid deposit response");
    println!("Transaction ID: {}", deposit.transaction_id);

    // 3. Wrap any fallible call with exponential-backoff retry.
    let config = RetryConfig::default(); // 3 attempts, 100 ms base, ×2
    let result = retry_with_backoff(
        &config,
        |_attempt| -> Result<&str, u32> { Ok("success") },
        |_err| false,
        |_ms| {},
    );
    assert_eq!(result, Ok("success"));
}
```

## Building the WASM Contract

```bash
cd SorobanAnchor
cargo build --release --target wasm32-unknown-unknown
```

The compiled WASM will be at:
`target/wasm32-unknown-unknown/release/anchorkit.wasm`

## Running Tests

```bash
cargo test
```

## Next Steps

- [SEP-6 Integration](sep6-integration.md)
- [SEP-10 Authentication](sep10-authentication.md)
- [SEP-24 Interactive Flows](sep24-interactive-flows.md)
- [SEP-38 Quotes](sep38-quotes.md)
- [Anchor Routing](anchor-routing.md)
- [Contract Deployment](contract-deployment.md)
