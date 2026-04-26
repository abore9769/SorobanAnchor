# SEP-6 Integration

[SEP-6](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0006.md)
defines a non-interactive protocol for depositing and withdrawing assets via an
anchor. AnchorKit's `sep6` module normalises raw anchor HTTP responses into
typed Rust structs.

## Overview

The SEP-6 flow is:

1. User calls the anchor's `/deposit` or `/withdraw` endpoint.
2. Anchor returns a JSON response with instructions.
3. AnchorKit normalises the response via `initiate_deposit` / `initiate_withdrawal`.
4. User follows the instructions (e.g. sends a bank transfer).
5. User polls the anchor's `/transaction` endpoint for status updates.
6. AnchorKit normalises status responses via `fetch_transaction_status`.

## Initiating a Deposit

```rust
use anchorkit::sep6::{initiate_deposit, RawDepositResponse, TransactionStatus};

// Populate from the anchor's HTTP response JSON.
let raw = RawDepositResponse {
    transaction_id: "txn-001".into(),
    how: "Send USD to bank account 1234, routing 021000021".into(),
    extra_info: Some("Reference: STELLAR-txn-001".into()),
    min_amount: Some(10),
    max_amount: Some(10_000),
    fee_fixed: Some(1),
    status: Some("pending_user_transfer_start".into()),
    clawback_enabled: None,
    stellar_memo: Some("12345".into()),
    stellar_memo_type: Some("id".into()),
};

let deposit = initiate_deposit(raw).expect("invalid deposit response");

println!("Transaction ID : {}", deposit.transaction_id);
println!("Instructions   : {}", deposit.how);
println!("Status         : {}", deposit.status.as_str());
// Status: pending_user
```

## Initiating a Withdrawal

```rust
use anchorkit::sep6::{initiate_withdrawal, RawWithdrawalResponse};

let raw = RawWithdrawalResponse {
    transaction_id: "txn-002".into(),
    account_id: "GABC123STELLARADDRESS".into(),
    memo: Some("12345".into()),
    memo_type: Some("id".into()),
    min_amount: Some(5),
    max_amount: Some(5_000),
    fee_fixed: Some(2),
    status: Some("pending_user".into()),
};

let withdrawal = initiate_withdrawal(raw).expect("invalid withdrawal response");
println!("Send to: {} (memo: {:?})", withdrawal.account_id, withdrawal.memo);
```

## Polling Transaction Status

```rust
use anchorkit::sep6::{fetch_transaction_status, RawTransactionResponse, TransactionStatus};

let raw = RawTransactionResponse {
    transaction_id: "txn-001".into(),
    kind: Some("deposit".into()),
    status: "completed".into(),
    amount_in: Some(100),
    amount_out: Some(99),
    amount_fee: Some(1),
    message: None,
};

let status = fetch_transaction_status(raw).expect("invalid status response");

match status.status {
    TransactionStatus::Completed => println!("Deposit complete!"),
    TransactionStatus::PendingExternal => println!("Waiting for bank transfer…"),
    TransactionStatus::Error => println!("Something went wrong"),
    other => println!("Status: {}", other.as_str()),
}
```

## Batch Status Polling

```rust
use anchorkit::sep6::{list_transactions, RawTransactionResponse};

let raw_list: Vec<RawTransactionResponse> = vec![/* ... */];
let transactions = list_transactions(raw_list);
// Entries with empty transaction_id are silently skipped.
println!("Active transactions: {}", transactions.len());
```

## Error Handling

All SEP-6 functions return `Result<_, AnchorKitError>`. The most common error
is `ErrorCode::InvalidTransactionIntent` when required fields are missing:

```rust
use anchorkit::{sep6::{initiate_deposit, RawDepositResponse}, ErrorCode};

let raw = RawDepositResponse {
    transaction_id: "".into(), // missing!
    how: "bank transfer".into(),
    // ...
    extra_info: None, min_amount: None, max_amount: None,
    fee_fixed: None, status: None, clawback_enabled: None,
    stellar_memo: None, stellar_memo_type: None,
};

let err = initiate_deposit(raw).unwrap_err();
assert_eq!(err.code, ErrorCode::InvalidTransactionIntent);
```

## Retry on Transient Failures

Wrap anchor HTTP calls with `retry_with_backoff` for resilience:

```rust
use anchorkit::retry::{retry_with_backoff, is_retryable, RetryConfig};
use anchorkit::ErrorCode;

let config = RetryConfig::new(3, 200, 5_000, 2);

let result = retry_with_backoff(
    &config,
    |_attempt| {
        // call_anchor_deposit_endpoint()
        Ok::<_, u32>(42)
    },
    |&code| is_retryable(code),
    |ms| std::thread::sleep(std::time::Duration::from_millis(ms)),
);
```
