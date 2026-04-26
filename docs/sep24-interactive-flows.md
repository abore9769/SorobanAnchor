# SEP-24 Interactive Flows

[SEP-24](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0024.md)
defines an interactive protocol where the anchor presents a web UI for the user
to complete KYC, select amounts, and confirm transactions. AnchorKit's `sep24`
module normalises the anchor's HTTP responses into typed Rust structs.

## Overview

The SEP-24 flow is:

1. Client calls the anchor's `/transactions/deposit/interactive` or
   `/transactions/withdraw/interactive` endpoint.
2. Anchor returns a `url` and `id`.
3. Client redirects the user to `url` (the anchor's interactive web UI).
4. User completes the flow in the browser.
5. Client polls `/transaction?id=<id>` for status updates.

## Initiating an Interactive Deposit

```rust
use anchorkit::sep24::{initiate_interactive_deposit, RawInteractiveDepositResponse};

// Populate from the anchor's HTTP response.
let raw = RawInteractiveDepositResponse {
    url: "https://anchor.example.com/sep24/deposit?token=abc123".into(),
    id: "tx-001".into(),
};

let resp = initiate_interactive_deposit(raw).expect("invalid response");

// Redirect the user to this URL.
println!("Redirect to: {}", resp.url);
println!("Transaction ID: {}", resp.id);
```

## Initiating an Interactive Withdrawal

```rust
use anchorkit::sep24::{initiate_interactive_withdrawal, RawInteractiveWithdrawalResponse};

let raw = RawInteractiveWithdrawalResponse {
    url: "https://anchor.example.com/sep24/withdraw?token=xyz789".into(),
    id: "tx-002".into(),
};

let resp = initiate_interactive_withdrawal(raw).expect("invalid response");
println!("Redirect to: {}", resp.url);
```

## Polling Transaction Status

```rust
use anchorkit::sep24::{fetch_sep24_transaction_status, RawSep24TransactionResponse};
use anchorkit::TransactionStatus;

let raw = RawSep24TransactionResponse {
    id: "tx-001".into(),
    status: "pending_user_transfer_start".into(),
    more_info_url: Some("https://anchor.example.com/tx/tx-001".into()),
    stellar_transaction_id: None,
};

let status = fetch_sep24_transaction_status(raw).expect("invalid response");

match status.status {
    TransactionStatus::Completed => {
        println!("Done! Stellar TX: {:?}", status.stellar_transaction_id);
    }
    TransactionStatus::PendingUser => {
        println!("Waiting for user action. More info: {:?}", status.more_info_url);
    }
    other => println!("Status: {}", other.as_str()),
}
```

## SEP-24 vs SEP-6

| Feature | SEP-6 | SEP-24 |
|---------|-------|--------|
| User interaction | Non-interactive (API only) | Interactive (web UI redirect) |
| KYC collection | Via API fields | Via anchor's web UI |
| `more_info_url` | Not present | Present in status response |
| `stellar_transaction_id` | Not present | Present when settled |

## Error Handling

Both `initiate_interactive_deposit` and `initiate_interactive_withdrawal` return
`Err(AnchorKitError)` with code `ErrorCode::ValidationError` when required
fields are missing:

```rust
use anchorkit::{
    sep24::{initiate_interactive_deposit, RawInteractiveDepositResponse},
    ErrorCode,
};

let raw = RawInteractiveDepositResponse {
    url: "".into(), // missing!
    id: "tx-001".into(),
};

let err = initiate_interactive_deposit(raw).unwrap_err();
assert_eq!(err.code, ErrorCode::ValidationError);
```

## Retry Pattern

```rust
use anchorkit::retry::{retry_with_backoff, RetryConfig};

let config = RetryConfig::new(3, 500, 10_000, 2);

let result = retry_with_backoff(
    &config,
    |_attempt| {
        // poll_sep24_status("tx-001")
        Ok::<_, u32>("completed")
    },
    |_err| true,
    |ms| std::thread::sleep(std::time::Duration::from_millis(ms)),
);
```
