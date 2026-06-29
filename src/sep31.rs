//! SEP-31 Direct Payment Service Layer
//!
//! Provides normalized service functions for initiating direct payments
//! across anchors implementing SEP-31.

extern crate alloc;
use alloc::string::String;

use crate::errors::Error;
use crate::response_validator::validate_stellar_account_id;

/// Raw fields from an anchor's direct payment initiation response.
pub struct RawSep31PaymentResponse {
    pub id: String,
    pub stellar_account_id: String,
    pub stellar_memo: Option<String>,
    pub stellar_memo_type: Option<String>,
}

/// Validated direct payment response from a SEP-31 anchor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sep31PaymentResponse {
    pub id: String,
    pub stellar_account_id: String,
    pub stellar_memo: Option<String>,
    pub stellar_memo_type: Option<String>,
}

/// Valid SEP-31 memo type strings.
const VALID_MEMO_TYPES: &[&str] = &["text", "id", "hash"];

/// Validate that whenever a memo value is present, a valid memo type is also present.
fn validate_memo_pair(memo: Option<&str>, memo_type: Option<&str>) -> Result<(), Error> {
    if memo.is_some() {
        match memo_type {
            None => return Err(Error::invalid_transaction_intent()),
            Some(mt) if !VALID_MEMO_TYPES.contains(&mt) => {
                return Err(Error::invalid_transaction_intent());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Normalize a raw SEP-31 direct payment response into a canonical
/// [`Sep31PaymentResponse`].
///
/// Validates that `id` is non-empty, `stellar_account_id` is a valid Stellar
/// account, and memo fields are consistent when present.
pub fn initiate_sep31_payment(
    raw: RawSep31PaymentResponse,
) -> Result<Sep31PaymentResponse, Error> {
    if raw.id.is_empty() {
        return Err(Error::invalid_transaction_intent());
    }
    validate_stellar_account_id(&raw.stellar_account_id)?;
    validate_memo_pair(
        raw.stellar_memo.as_deref(),
        raw.stellar_memo_type.as_deref(),
    )?;

    Ok(Sep31PaymentResponse {
        id: raw.id,
        stellar_account_id: raw.stellar_account_id,
        stellar_memo: raw.stellar_memo,
        stellar_memo_type: raw.stellar_memo_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    const VALID_ACCOUNT: &str =
        "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5";

    fn raw_payment() -> RawSep31PaymentResponse {
        RawSep31PaymentResponse {
            id: "pay-001".to_string(),
            stellar_account_id: VALID_ACCOUNT.to_string(),
            stellar_memo: None,
            stellar_memo_type: None,
        }
    }

    #[test]
    fn test_initiate_sep31_payment_accepts_valid_response() {
        let resp = initiate_sep31_payment(raw_payment()).unwrap();
        assert_eq!(resp.id, "pay-001");
        assert_eq!(resp.stellar_account_id, VALID_ACCOUNT);
    }

    #[test]
    fn test_initiate_sep31_payment_rejects_empty_id() {
        let mut raw = raw_payment();
        raw.id = String::new();
        assert_eq!(
            initiate_sep31_payment(raw),
            Err(Error::invalid_transaction_intent())
        );
    }

    #[test]
    fn test_initiate_sep31_payment_rejects_invalid_account_id() {
        let mut raw = raw_payment();
        raw.stellar_account_id = "not-a-valid-account".to_string();
        assert!(initiate_sep31_payment(raw).is_err());
    }

    #[test]
    fn test_initiate_sep31_payment_rejects_memo_without_type() {
        let mut raw = raw_payment();
        raw.stellar_memo = Some("12345".to_string());
        raw.stellar_memo_type = None;
        assert_eq!(
            initiate_sep31_payment(raw),
            Err(Error::invalid_transaction_intent())
        );
    }

    #[test]
    fn test_initiate_sep31_payment_rejects_invalid_memo_type() {
        let mut raw = raw_payment();
        raw.stellar_memo = Some("12345".to_string());
        raw.stellar_memo_type = Some("fax".to_string());
        assert_eq!(
            initiate_sep31_payment(raw),
            Err(Error::invalid_transaction_intent())
        );
    }
}
