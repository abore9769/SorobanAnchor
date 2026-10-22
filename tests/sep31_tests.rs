//! Tests for SEP-31 direct payment support (#558).

use anchorkit::contract::{
    AnchorKitContract, AnchorKitContractClient, ServiceType, SERVICE_SEP31,
};
use anchorkit::sep31::{initiate_sep31_payment, RawSep31PaymentResponse};
use anchorkit::Error;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[path = "sep10_test_util.rs"]
mod sep10_test_util;

use sep10_test_util::register_attestor_with_sep10;

const VALID_ACCOUNT: &str = "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5";

fn raw_payment() -> RawSep31PaymentResponse {
    RawSep31PaymentResponse {
        id: "pay-001".into(),
        stellar_account_id: VALID_ACCOUNT.into(),
        sender: "sender-001".into(),
        stellar_memo: None,
        stellar_memo_type: None,
        amount: None,
        asset_code: None,
        idempotency_key: None,
    }
}

#[test]
fn valid_payment_response_accepted() {
    let resp = initiate_sep31_payment(raw_payment()).unwrap();
    assert_eq!(resp.id, "pay-001");
    assert_eq!(resp.stellar_account_id, VALID_ACCOUNT);
}

#[test]
fn empty_id_rejected() {
    let mut raw = raw_payment();
    raw.id.clear();
    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent())
    );
}

#[test]
fn empty_sender_rejected() {
    let mut raw = raw_payment();
    raw.sender.clear();
    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent())
    );
}

#[test]
fn invalid_stellar_account_id_rejected() {
    let mut raw = raw_payment();
    raw.stellar_account_id = "invalid-account".into();
    assert!(initiate_sep31_payment(raw).is_err());
}

#[test]
fn memo_without_memo_type_rejected() {
    let mut raw = raw_payment();
    raw.stellar_memo = Some("12345".into());
    raw.stellar_memo_type = None;
    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent())
    );
}

#[test]
fn memo_with_invalid_type_rejected() {
    let mut raw = raw_payment();
    raw.stellar_memo = Some("12345".into());
    raw.stellar_memo_type = Some("fax".into());
    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent())
    );
}

// -----------------------------------------------------------------------
// Task 1: validate_positive_decimal — canonical decimal grammar
// Leading/trailing decimal points are not part of the canonical grammar.
// -----------------------------------------------------------------------

#[test]
fn amount_leading_dot_rejected() {
    // ".5" is an ambiguous form; the canonical grammar requires an integer part.
    let mut raw = raw_payment();
    raw.amount = Some(".5".into());
    assert!(
        initiate_sep31_payment(raw).is_err(),
        "leading dot (.5) must be rejected"
    );
}

#[test]
fn amount_trailing_dot_rejected() {
    // "5." is an ambiguous form; the fractional part, when present, must
    // contain at least one digit.
    let mut raw = raw_payment();
    raw.amount = Some("5.".into());
    assert!(
        initiate_sep31_payment(raw).is_err(),
        "trailing dot (5.) must be rejected"
    );
}

#[test]
fn amount_canonical_zero_point_five_accepted() {
    // "0.5" is the canonical way to express .5 and must be accepted.
    let mut raw = raw_payment();
    raw.amount = Some("0.5".into());
    let resp = initiate_sep31_payment(raw).unwrap();
    assert_eq!(resp.amount.as_deref(), Some("0.5"));
}

// -----------------------------------------------------------------------
// Task 3: whitespace-only transaction ID must be rejected.
// -----------------------------------------------------------------------

#[test]
fn whitespace_only_id_rejected() {
    let mut raw = raw_payment();
    raw.id = "   ".into();
    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent()),
        "whitespace-only id must be rejected before any payment state is written"
    );
}

#[test]
fn tab_only_id_rejected() {
    let mut raw = raw_payment();
    raw.id = "\t\n".into();
    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent()),
    );
}

// -----------------------------------------------------------------------
// Task 4: validate_positive_decimal — zero values must be rejected.
// The field contract says "positive decimal"; zero does not satisfy that.
// -----------------------------------------------------------------------

#[test]
fn amount_zero_rejected() {
    let mut raw = raw_payment();
    raw.amount = Some("0".into());
    assert!(
        initiate_sep31_payment(raw).is_err(),
        "zero amount must be rejected"
    );
}

#[test]
fn amount_zero_decimal_rejected() {
    let mut raw = raw_payment();
    raw.amount = Some("0.00".into());
    assert!(
        initiate_sep31_payment(raw).is_err(),
        "0.00 must be rejected as numerically zero"
    );
}

#[test]
fn amount_zero_many_decimals_rejected() {
    let mut raw = raw_payment();
    raw.amount = Some("0.000000000".into());
    assert!(
        initiate_sep31_payment(raw).is_err(),
        "0.000000000 must be rejected as numerically zero"
    );
}

#[test]
fn amount_fractional_positive_accepted() {
    // A non-zero fractional value must still be accepted.
    let mut raw = raw_payment();
    raw.amount = Some("0.01".into());
    let resp = initiate_sep31_payment(raw).unwrap();
    assert_eq!(resp.amount.as_deref(), Some("0.01"));
}

#[test]
fn amount_positive_integer_accepted() {
    let mut raw = raw_payment();
    raw.amount = Some("1".into());
    assert!(initiate_sep31_payment(raw).is_ok());
}

// -----------------------------------------------------------------------
// Optional party fields — minimal and populated fixtures.
//
// Cross-border payment payloads vary by corridor and anchor.  Tests here
// confirm that optional KYC-adjacent fields (memo, asset code, amount,
// idempotency key) are never treated as universally required: a request
// with none of them present must succeed, and a request with all of them
// present must preserve every provided value.
//
// Synthetic identifiers are used throughout; no real personal data appears.
// -----------------------------------------------------------------------

/// Minimal fixture: only the three mandatory fields are populated.
/// Every optional field is None.  The parser must accept this shape without
/// error and must not fabricate values for absent fields.
#[test]
fn optional_party_fields_absent_shape_accepted() {
    let raw = RawSep31PaymentResponse {
        id:                  "pay-minimal-001".into(),
        stellar_account_id:  VALID_ACCOUNT.into(),
        sender:              "sender-synthetic-001".into(),
        stellar_memo:        None,
        stellar_memo_type:   None,
        amount:              None,
        asset_code:          None,
        idempotency_key:     None,
    };

    let resp = initiate_sep31_payment(raw).unwrap();

    // Required fields are preserved.
    assert_eq!(resp.id,                 "pay-minimal-001");
    assert_eq!(resp.stellar_account_id, VALID_ACCOUNT);
    assert_eq!(resp.sender,             "sender-synthetic-001");

    // Every optional field must remain absent — no defaults injected.
    assert!(resp.stellar_memo.is_none(),      "memo must be None when not provided");
    assert!(resp.stellar_memo_type.is_none(), "memo_type must be None when not provided");
    assert!(resp.amount.is_none(),            "amount must be None when not provided");
    assert!(resp.asset_code.is_none(),        "asset_code must be None when not provided");
    assert!(resp.idempotency_key.is_none(),   "idempotency_key must be None when not provided");
}

/// Populated fixture: all optional fields are supplied with synthetic values.
/// Each provided value must be preserved (with asset code normalised to
/// uppercase per the field contract).
#[test]
fn optional_party_fields_present_shape_values_preserved() {
    let raw = RawSep31PaymentResponse {
        id:                  "pay-full-001".into(),
        stellar_account_id:  VALID_ACCOUNT.into(),
        sender:              "sender-synthetic-002".into(),
        stellar_memo:        Some("synth-memo-42".into()),
        stellar_memo_type:   Some("text".into()),
        amount:              Some("250.75".into()),
        asset_code:          Some("usdc".into()),   // mixed-case; must be normalised
        idempotency_key:     Some("idem-synth-xyz-001".into()),
    };

    let resp = initiate_sep31_payment(raw).unwrap();

    // Required fields.
    assert_eq!(resp.id,                 "pay-full-001");
    assert_eq!(resp.stellar_account_id, VALID_ACCOUNT);
    assert_eq!(resp.sender,             "sender-synthetic-002");

    // Optional fields — values must be preserved exactly (asset code uppercased).
    assert_eq!(resp.stellar_memo.as_deref(),      Some("synth-memo-42"),        "memo must be preserved");
    assert_eq!(resp.stellar_memo_type.as_deref(), Some("text"),                  "memo_type must be preserved");
    assert_eq!(resp.amount.as_deref(),            Some("250.75"),                "amount must be preserved");
    assert_eq!(resp.asset_code.as_deref(),        Some("USDC"),                  "asset_code must be normalised to uppercase");
    assert_eq!(resp.idempotency_key.as_deref(),   Some("idem-synth-xyz-001"),    "idempotency_key must be preserved");
}

/// Required-field validation still fires even when all optional fields are
/// absent.  An empty id with no optional fields must still be rejected.
#[test]
fn required_field_validation_still_enforced_with_all_optionals_absent() {
    let raw = RawSep31PaymentResponse {
        id:                  "".into(),   // violates required constraint
        stellar_account_id:  VALID_ACCOUNT.into(),
        sender:              "sender-synthetic-003".into(),
        stellar_memo:        None,
        stellar_memo_type:   None,
        amount:              None,
        asset_code:          None,
        idempotency_key:     None,
    };

    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent()),
        "empty id must be rejected regardless of optional field state",
    );
}

/// Required-field validation still fires even when all optional fields are
/// populated.  An empty sender in an otherwise-complete payload must be
/// rejected, confirming optional presence does not mask required-field errors.
#[test]
fn required_field_validation_still_enforced_with_all_optionals_present() {
    let raw = RawSep31PaymentResponse {
        id:                  "pay-validation-check".into(),
        stellar_account_id:  VALID_ACCOUNT.into(),
        sender:              "   ".into(), // whitespace-only: violates required constraint
        stellar_memo:        Some("memo-val".into()),
        stellar_memo_type:   Some("text".into()),
        amount:              Some("10.00".into()),
        asset_code:          Some("XLM".into()),
        idempotency_key:     Some("idem-check-001".into()),
    };

    assert_eq!(
        initiate_sep31_payment(raw),
        Err(Error::invalid_transaction_intent()),
        "whitespace-only sender must be rejected even when all optional fields are present",
    );
}

#[test]
fn service_type_sep31_detected_in_capability_check() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let anchor = Address::generate(&env);
    let sk = SigningKey::generate(&mut OsRng);
    register_attestor_with_sep10(&env, &client, &anchor, &anchor, &sk);

    let mut services = Vec::new(&env);
    services.push_back(ServiceType::Sep31.as_u32());

    client.configure_services(&anchor, &services);

    assert_eq!(ServiceType::Sep31.as_u32(), SERVICE_SEP31);
    assert!(client.supports_service(&anchor, &SERVICE_SEP31));
}
