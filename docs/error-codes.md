# AnchorKit Error Code Reference

All errors are represented as `AnchorKitError` (see `src/errors.rs`), carrying a
numeric `ErrorCode`, a human-readable `message`, and an optional `context` string.

## Numbering scheme

| Range  | Category                              |
|--------|---------------------------------------|
|  1–10  | Auth / attestor errors                |
| 11–19  | Validation / quote / flow errors      |
| 20–29  | KYC / webhook / state errors          |
| 48–49  | Cache errors                          |
| 50–52  | Profile / metadata validation errors  |

---

## Auth / attestor errors (1–10)

| Code | Variant                    | Message                                      | Raised by                                      |
|------|----------------------------|----------------------------------------------|------------------------------------------------|
| 1    | `AlreadyInitialized`       | Contract is already initialized              | `initialize` called twice                      |
| 2    | `AttestorAlreadyRegistered`| Attestor is already registered               | `register_attestor` with duplicate address     |
| 3    | `AttestorNotRegistered`    | Attestor is not registered                   | Any attestor-gated call with unknown address   |
| 4    | `UnauthorizedAttestor`     | Attestor is not authorized                   | Signature mismatch on attestation              |
| 5    | `InvalidTimestamp`         | Timestamp is invalid                         | Attestation timestamp too old/future           |
| 6    | `ReplayAttack`             | Replay attack detected                       | Duplicate `(issuer, payload_hash)` pair        |
| 7    | `InvalidQuote`             | Quote is invalid                             | `submit_quote` with bad fee or amounts         |
| 8    | `InvalidServiceType`       | Service type is invalid                      | `configure_services` with unknown/duplicate    |
| 9    | `InvalidTransactionIntent` | Transaction intent is invalid                | SEP-6/24 intent validation                     |
| 10   | `StaleQuote`               | Quote has expired                            | `route_transaction` with expired quote         |

---

## Validation / quote / flow errors (11–19)

| Code | Variant                  | Message                                      | Raised by                                      |
|------|--------------------------|----------------------------------------------|------------------------------------------------|
| 11   | `ComplianceNotMet`       | Compliance requirements not met              | `route_transaction` with `require_compliance`  |
| 12   | `InvalidEndpointFormat`  | Endpoint format is invalid                   | `set_endpoint`, `register_webhook` (non-HTTPS) |
| 13   | `NoQuotesAvailable`      | No quotes are available                      | `route_transaction` with no matching quotes    |
| 14   | `ServicesNotConfigured`  | Services are not configured                  | `get_supported_services` before configuration  |
| 15   | `ValidationError`        | Response schema validation failed            | Schema checks, empty strings, bad lengths      |
| 16   | `RateLimitExceeded`      | Rate limit exceeded                          | `submit_attestation` over per-attestor limit   |
| 17   | `AttestationNotFound`    | Attestation not found                        | `get_attestation` with unknown ID              |
| 18   | `InvalidSep10Token`      | SEP-10 JWT is missing, expired, or invalid   | `verify_sep10_token*` family                   |
| 19   | `KycNotFound`            | KYC record not found                         | `approve_kyc`, `reject_kyc` before `submit_kyc`|

---

## KYC / webhook / state errors (20–29)

| Code | Variant                  | Message                                      | Raised by                                      |
|------|--------------------------|----------------------------------------------|------------------------------------------------|
| 20   | `KycPending`             | KYC verification is pending                  | `submit_attestation_kyc_check` (pending KYC)   |
| 21   | `KycRejected`            | KYC verification was rejected                | `submit_attestation_kyc_check` (rejected KYC)  |
| 22   | `WebhookDeliveryFailed`  | Webhook delivery failed validation           | `deliver_webhook` after max retries            |
| 23   | `NotInitialized`         | Contract is not initialized                  | Any call before `initialize`                   |
| 24   | `IllegalTransition`      | Illegal transaction state transition         | `TransactionStateTracker::transition`          |
| 25   | `SessionExpired`         | Session has expired                          | Session-gated calls after TTL                  |
| 26   | `SessionClosed`          | Session is closed                            | Session-gated calls on a closed session        |

---

## Cache errors (48–49)

| Code | Variant        | Message                    | Raised by                                          |
|------|----------------|----------------------------|----------------------------------------------------|
| 48   | `CacheExpired` | Cache entry has expired    | `get_cached_metadata`, `get_cached_capabilities`   |
| 49   | `CacheNotFound`| Cache entry not found      | `get_cached_metadata`, `get_cached_capabilities`   |

---

## Profile / metadata validation errors (50–52)

| Code | Variant                    | Message                              | Raised by                                          |
|------|----------------------------|--------------------------------------|----------------------------------------------------|
| 50   | `AttestorProfileNotFound`  | Attestor profile not found           | `get_attestor_profile` before any profile write    |
| 51   | `InvalidRequestContext`    | Request context is invalid           | `create_request_context` with empty/zero fields    |
| 52   | `InvalidSessionMetadata`   | Session metadata is invalid          | Session creation with malformed operation context  |

---

## Usage

```rust
use anchorkit::{AnchorKitError, ErrorCode};

// Named constructor (preferred)
let err = AnchorKitError::replay_attack();
assert_eq!(err.code, ErrorCode::ReplayAttack);  // code = 6

// With context detail
let err = AnchorKitError::validation_error("field: operation_type");
assert_eq!(err.code as u32, 15);

// New profile error
let err = AnchorKitError::attestor_profile_not_found();
assert_eq!(err.code as u32, 50);
```
