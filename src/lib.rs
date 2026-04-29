//! # AnchorKit
//!
//! AnchorKit is a Soroban smart-contract library for building and interacting with
//! Stellar anchor services. It provides both an on-chain contract layer and an
//! off-chain service layer that normalises responses from anchors implementing the
//! [Stellar Ecosystem Proposals (SEPs)](https://github.com/stellar/stellar-protocol/tree/master/ecosystem).
//!
//! ## Architecture
//!
//! The library is split into two logical layers:
//!
//! ### On-chain contract layer (`contract` module)
//! The [`contract::AnchorKitContract`] Soroban contract manages:
//! - Attestor registration and revocation (with SEP-10 JWT verification)
//! - Attestation submission with replay protection and rate limiting
//! - Session-based multi-step operations with audit logging
//! - Quote routing across multiple anchors
//! - KYC / compliance record tracking
//! - Anchor metadata and capability caching
//! - Anchor discovery via `stellar.toml`
//!
//! ### Off-chain service layer (SEP modules)
//! Three thin normalisation modules translate raw anchor HTTP responses into
//! typed Rust structs so callers never have to parse raw JSON themselves:
//!
//! | Module | SEP | Purpose |
//! |--------|-----|---------|
//! | [`sep6`] | SEP-6 | Non-interactive deposit / withdrawal |
//! | [`sep24`] | SEP-24 | Interactive deposit / withdrawal |
//! | `sep38` (internal) | SEP-38 | Anchor RFQ / firm quotes |
//!
//! ### Cross-cutting utilities
//! | Module | Purpose |
//! |--------|---------|
//! | `domain_validator` | HTTPS-only URL validation before any outbound request |
//! | `errors` | Unified [`AnchorKitError`] / [`ErrorCode`] type hierarchy |
//! | `rate_limiter` | Per-attestor sliding-window rate limiting |
//! | `retry` | Exponential-backoff retry for transient failures |
//! | `sep10_jwt` | EdDSA JWT verification (SEP-10 authentication) |
//! | `deterministic_hash` | Canonical SHA-256 hashing for attestation payloads |
//! | `transaction_state_tracker` | State-machine tracking for on-chain transactions |
//! | `response_validator` | Schema validation for anchor API responses |
//!
//! ## Quick-start example
//!
//! ```rust,no_run
//! use anchorkit::{
//!     validate_anchor_domain,
//!     sep6::{initiate_deposit, RawDepositResponse},
//!     sep24::{initiate_interactive_deposit, RawInteractiveDepositResponse},
//!     retry::{retry_with_backoff, RetryConfig},
//! };
//!
//! // 1. Validate the anchor domain before making any requests.
//! validate_anchor_domain("https://anchor.example.com").expect("invalid domain");
//!
//! // 2. Normalise a SEP-6 deposit response received from the anchor's HTTP API.
//! let raw = RawDepositResponse {
//!     transaction_id: "txn-001".into(),
//!     how: "Send to bank account 1234".into(),
//!     extra_info: None,
//!     min_amount: Some(10),
//!     max_amount: Some(10_000),
//!     fee_fixed: Some(1),
//!     status: Some("pending_external".into()),
//!     clawback_enabled: None,
//!     stellar_memo: None,
//!     stellar_memo_type: None,
//! };
//! let deposit = initiate_deposit(raw).expect("invalid deposit response");
//! println!("Transaction ID: {}", deposit.transaction_id);
//!
//! // 3. Normalise a SEP-24 interactive deposit response.
//! let raw24 = RawInteractiveDepositResponse {
//!     url: "https://anchor.example.com/interactive/deposit".into(),
//!     id: "txn-002".into(),
//! };
//! let interactive = initiate_interactive_deposit(raw24).expect("invalid response");
//! println!("Redirect user to: {}", interactive.url);
//!
//! // 4. Wrap any fallible call with exponential-backoff retry.
//! let config = RetryConfig::default();
//! let result = retry_with_backoff(
//!     &config,
//!     |_attempt| -> Result<&str, u32> { Ok("success") },
//!     |_err| false,
//!     |_ms| {},
//! );
//! assert_eq!(result, Ok("success"));
//! ```
//!
//! ## Feature flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `std` | ✓ | Enable standard-library support |
//! | `wasm` | — | Target Soroban WASM environment |
//! | `mock-only` | — | Compile only mock/test helpers |
//! | `stress-tests` | — | Enable load-simulation test suite |

#![no_std]
extern crate alloc;

mod deterministic_hash;
mod domain_validator;
mod errors;
pub mod sep10_jwt;
mod rate_limiter;
mod response_validator;
mod retry;
mod transaction_state_tracker;
pub mod webhook;
pub mod sep6;
pub mod sep24;
pub mod sep38;
pub mod contract;
pub mod stellar_toml;

pub use domain_validator::validate_anchor_domain;
pub use errors::{AnchorKitError, ErrorCode};
pub use stellar_toml::{ParsedStellarToml, parse_stellar_toml, fetch_stellar_toml_url};

/// Backward-compatible alias. Prefer [`AnchorKitError`] for new code.
pub use errors::Error;
pub use rate_limiter::{RateLimiter, RateLimitConfig, RateLimitState};
pub use response_validator::{
    validate_anchor_info_response, validate_deposit_response, validate_quote_response,
    validate_sep38_quote_response, validate_withdraw_response, validate_stellar_asset,
    AnchorInfoResponse, DepositResponse as ValidatorDepositResponse, QuoteResponse,
    Sep38QuoteResponse, WithdrawResponse,
};
pub use retry::{retry_with_backoff, is_retryable, RetryConfig, JitterSource, LedgerJitterSource, MockJitterSource};
pub use deterministic_hash::{compute_payload_hash, verify_payload_hash};
pub use webhook::{deliver_webhook, get_dead_letter_webhooks, WebhookDeliveryConfig};

#[cfg(test)]
mod transaction_state_tracker_tests;
pub use sep6::{
    fetch_transaction_status, initiate_deposit, initiate_withdrawal, DepositResponse,
    RawDepositResponse, RawTransactionResponse, RawWithdrawalResponse, TransactionKind,
    TransactionStatus, TransactionStatusResponse, WithdrawalResponse,
    poll_transaction_status, PollConfig, PollResult,
};
pub use sep24::{
    initiate_interactive_deposit, initiate_interactive_withdrawal, fetch_sep24_transaction_status,
    validate_interactive_url, validate_transaction_id,
    InteractiveDepositResponse, InteractiveWithdrawalResponse, Sep24TransactionStatusResponse,
    RawInteractiveDepositResponse, RawInteractiveWithdrawalResponse, RawSep24TransactionResponse,
};
pub use contract::{AnchorKitContract, EndpointUpdated, get_endpoint, set_endpoint};
pub use transaction_state_tracker::{TransactionState, TransactionStateRecord};
pub use transaction_state_tracker::StorageBudgetMonitor;
pub mod streaming_monitor;
pub use streaming_monitor::{StreamingTransactionMonitor, TransactionStatusUpdate};

#[cfg(test)]
mod request_id_tests;

#[cfg(test)]
mod tracing_span_tests;

#[cfg(test)]
mod metadata_cache_tests;

#[cfg(test)]
mod streaming_flow_tests;

#[cfg(test)]
mod webhook_middleware_tests;

#[cfg(test)]
mod session_tests;

#[cfg(test)]
mod sep10_test_util;

#[cfg(test)]
mod sep10_contract_tests;

#[cfg(test)]
mod routing_tests;

#[cfg(test)]
mod attestation_sig_tests;

#[cfg(test)]
mod deterministic_hash_snapshot_tests {
    // Snapshot tests live inside deterministic_hash module itself.
    // This module exists to satisfy the test_snapshots/deterministic_hash_tests path.
}

mod capability_detection_tests;

#[cfg(test)]
mod attestor_endpoint_tests;

#[cfg(test)]
mod stellar_toml_tests;
