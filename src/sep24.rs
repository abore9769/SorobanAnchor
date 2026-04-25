//! SEP-24 Interactive Deposit & Withdrawal Service Layer
//!
//! Provides normalized service functions for initiating interactive deposits,
//! interactive withdrawals, and fetching transaction status for SEP-24 flows.

#![cfg_attr(not(test), no_std)]

extern crate alloc;
use alloc::string::{String, ToString};

use crate::domain_validator::validate_anchor_domain;
use crate::errors::{AnchorKitError, ErrorCode};
use crate::sep6::TransactionStatus;

/// Raw response from anchor's `/transactions/deposit/interactive` endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawInteractiveDepositResponse {
    pub url: String,
    pub id: String,
}

/// Raw response from anchor's `/transactions/withdraw/interactive` endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawInteractiveWithdrawalResponse {
    pub url: String,
    pub id: String,
}

/// Raw response from anchor's `/transaction` endpoint for SEP-24.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawSep24TransactionResponse {
    pub id: String,
    pub status: String,
    pub more_info_url: Option<String>,
    pub stellar_transaction_id: Option<String>,
}

/// Normalized response for interactive deposit initiation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractiveDepositResponse {
    /// URL to redirect user to for interactive flow.
    pub url: String,
    /// Unique transaction ID assigned by the anchor.
    pub id: String,
}

/// Normalized response for interactive withdrawal initiation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractiveWithdrawalResponse {
    /// URL to redirect user to for interactive flow.
    pub url: String,
    /// Unique transaction ID assigned by the anchor.
    pub id: String,
}

/// Normalized response for SEP-24 transaction status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sep24TransactionStatusResponse {
    /// Unique transaction ID.
    pub id: String,
    /// Current status of the transaction.
    pub status: TransactionStatus,
    /// URL with more information about the transaction (SEP-24 specific).
    pub more_info_url: Option<String>,
    /// Stellar transaction ID if available (SEP-24 specific).
    pub stellar_transaction_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Validates that a SEP-24 interactive flow URL is a well-formed HTTPS URL.
/// Delegates to `validate_anchor_domain` to avoid duplicating logic.
pub fn validate_interactive_url(url: &str) -> Result<(), AnchorKitError> {
    validate_anchor_domain(url).map_err(|_| AnchorKitError::invalid_endpoint_format())
}

/// Validates that a transaction ID is non-empty and contains only
/// alphanumeric characters, hyphens, and underscores.
pub fn validate_transaction_id(id: &str) -> Result<(), AnchorKitError> {
    if id.is_empty() {
        return Err(AnchorKitError::new(
            ErrorCode::ValidationError,
            "Transaction ID must not be empty",
        ));
    }
    for c in id.chars() {
        if !c.is_alphanumeric() && c != '-' && c != '_' {
            return Err(AnchorKitError::new(
                ErrorCode::ValidationError,
                "Transaction ID contains invalid characters",
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Service functions
// ---------------------------------------------------------------------------

/// Normalizes the anchor's `/transactions/deposit/interactive` response.
///
/// # Arguments
/// * `raw` - Raw response from the anchor
///
/// # Returns
/// Normalized `InteractiveDepositResponse` or an error
pub fn initiate_interactive_deposit(
    raw: RawInteractiveDepositResponse,
) -> Result<InteractiveDepositResponse, AnchorKitError> {
    validate_interactive_url(&raw.url)?;
    validate_transaction_id(&raw.id)?;
    Ok(InteractiveDepositResponse {
        url: raw.url,
        id: raw.id,
    })
}

/// Normalizes the anchor's `/transactions/withdraw/interactive` response.
///
/// # Arguments
/// * `raw` - Raw response from the anchor
///
/// # Returns
/// Normalized `InteractiveWithdrawalResponse` or an error
pub fn initiate_interactive_withdrawal(
    raw: RawInteractiveWithdrawalResponse,
) -> Result<InteractiveWithdrawalResponse, AnchorKitError> {
    validate_interactive_url(&raw.url)?;
    validate_transaction_id(&raw.id)?;
    Ok(InteractiveWithdrawalResponse {
        url: raw.url,
        id: raw.id,
    })
}

/// Normalizes the anchor's `/transaction` response for SEP-24 flows.
///
/// Maps SEP-24 specific fields like `more_info_url` and `stellar_transaction_id`.
///
/// # Arguments
/// * `raw` - Raw response from the anchor
///
/// # Returns
/// Normalized `Sep24TransactionStatusResponse` or an error
pub fn fetch_sep24_transaction_status(
    raw: RawSep24TransactionResponse,
) -> Result<Sep24TransactionStatusResponse, AnchorKitError> {
    if raw.id.is_empty() {
        return Err(AnchorKitError::new(
            ErrorCode::ValidationError,
            "Missing id field in SEP-24 transaction response",
        ));
    }
    if raw.status.is_empty() {
        return Err(AnchorKitError::new(
            ErrorCode::ValidationError,
            "Missing status field in SEP-24 transaction response",
        ));
    }
    if let Some(ref url) = raw.more_info_url {
        validate_interactive_url(url)?;
    }

    Ok(Sep24TransactionStatusResponse {
        id: raw.id,
        status: TransactionStatus::from_str(&raw.status),
        more_info_url: raw.more_info_url,
        stellar_transaction_id: raw.stellar_transaction_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // validate_interactive_url
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_interactive_url_accepts_https() {
        assert!(validate_interactive_url("https://anchor.example.com/deposit").is_ok());
    }

    #[test]
    fn test_validate_interactive_url_rejects_http() {
        assert!(validate_interactive_url("http://anchor.example.com/deposit").is_err());
    }

    #[test]
    fn test_validate_interactive_url_rejects_relative() {
        assert!(validate_interactive_url("/deposit/interactive").is_err());
        assert!(validate_interactive_url("deposit/interactive").is_err());
    }

    #[test]
    fn test_validate_interactive_url_rejects_data_uri() {
        assert!(validate_interactive_url("data:text/html,<h1>phish</h1>").is_err());
    }

    #[test]
    fn test_validate_interactive_url_rejects_empty() {
        assert!(validate_interactive_url("").is_err());
    }

    // -----------------------------------------------------------------------
    // validate_transaction_id
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_transaction_id_accepts_valid() {
        assert!(validate_transaction_id("tx-123").is_ok());
        assert!(validate_transaction_id("tx_abc_456").is_ok());
        assert!(validate_transaction_id("ABC123").is_ok());
    }

    #[test]
    fn test_validate_transaction_id_rejects_empty() {
        assert!(validate_transaction_id("").is_err());
    }

    #[test]
    fn test_validate_transaction_id_rejects_invalid_chars() {
        assert!(validate_transaction_id("tx 123").is_err());
        assert!(validate_transaction_id("tx/123").is_err());
        assert!(validate_transaction_id("tx@123").is_err());
    }

    // -----------------------------------------------------------------------
    // initiate_interactive_deposit
    // -----------------------------------------------------------------------

    #[test]
    fn test_initiate_interactive_deposit_success() {
        let raw = RawInteractiveDepositResponse {
            url: "https://anchor.example.com/deposit".to_string(),
            id: "tx-123".to_string(),
        };
        let result = initiate_interactive_deposit(raw).unwrap();
        assert_eq!(result.url, "https://anchor.example.com/deposit");
        assert_eq!(result.id, "tx-123");
    }

    #[test]
    fn test_initiate_interactive_deposit_rejects_http_url() {
        let raw = RawInteractiveDepositResponse {
            url: "http://anchor.example.com/deposit".to_string(),
            id: "tx-123".to_string(),
        };
        assert!(initiate_interactive_deposit(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_deposit_rejects_relative_url() {
        let raw = RawInteractiveDepositResponse {
            url: "/deposit/interactive".to_string(),
            id: "tx-123".to_string(),
        };
        assert!(initiate_interactive_deposit(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_deposit_rejects_data_uri() {
        let raw = RawInteractiveDepositResponse {
            url: "data:text/html,<h1>phish</h1>".to_string(),
            id: "tx-123".to_string(),
        };
        assert!(initiate_interactive_deposit(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_deposit_missing_url() {
        let raw = RawInteractiveDepositResponse {
            url: "".to_string(),
            id: "tx-123".to_string(),
        };
        assert!(initiate_interactive_deposit(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_deposit_missing_id() {
        let raw = RawInteractiveDepositResponse {
            url: "https://anchor.example.com/deposit".to_string(),
            id: "".to_string(),
        };
        assert!(initiate_interactive_deposit(raw).is_err());
    }

    // -----------------------------------------------------------------------
    // initiate_interactive_withdrawal
    // -----------------------------------------------------------------------

    #[test]
    fn test_initiate_interactive_withdrawal_success() {
        let raw = RawInteractiveWithdrawalResponse {
            url: "https://anchor.example.com/withdraw".to_string(),
            id: "tx-456".to_string(),
        };
        let result = initiate_interactive_withdrawal(raw).unwrap();
        assert_eq!(result.url, "https://anchor.example.com/withdraw");
        assert_eq!(result.id, "tx-456");
    }

    #[test]
    fn test_initiate_interactive_withdrawal_rejects_http_url() {
        let raw = RawInteractiveWithdrawalResponse {
            url: "http://anchor.example.com/withdraw".to_string(),
            id: "tx-456".to_string(),
        };
        assert!(initiate_interactive_withdrawal(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_withdrawal_rejects_relative_url() {
        let raw = RawInteractiveWithdrawalResponse {
            url: "/withdraw/interactive".to_string(),
            id: "tx-456".to_string(),
        };
        assert!(initiate_interactive_withdrawal(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_withdrawal_rejects_data_uri() {
        let raw = RawInteractiveWithdrawalResponse {
            url: "data:text/html,<h1>phish</h1>".to_string(),
            id: "tx-456".to_string(),
        };
        assert!(initiate_interactive_withdrawal(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_withdrawal_missing_url() {
        let raw = RawInteractiveWithdrawalResponse {
            url: "".to_string(),
            id: "tx-456".to_string(),
        };
        assert!(initiate_interactive_withdrawal(raw).is_err());
    }

    #[test]
    fn test_initiate_interactive_withdrawal_missing_id() {
        let raw = RawInteractiveWithdrawalResponse {
            url: "https://anchor.example.com/withdraw".to_string(),
            id: "".to_string(),
        };
        assert!(initiate_interactive_withdrawal(raw).is_err());
    }

    // -----------------------------------------------------------------------
    // fetch_sep24_transaction_status
    // -----------------------------------------------------------------------

    #[test]
    fn test_fetch_sep24_transaction_status_success() {
        let raw = RawSep24TransactionResponse {
            id: "tx-789".to_string(),
            status: "completed".to_string(),
            more_info_url: Some("https://anchor.example.com/tx/tx-789".to_string()),
            stellar_transaction_id: Some("stellar-tx-123".to_string()),
        };
        let result = fetch_sep24_transaction_status(raw).unwrap();
        assert_eq!(result.id, "tx-789");
        assert_eq!(result.status, TransactionStatus::Completed);
        assert_eq!(
            result.more_info_url,
            Some("https://anchor.example.com/tx/tx-789".to_string())
        );
        assert_eq!(
            result.stellar_transaction_id,
            Some("stellar-tx-123".to_string())
        );
    }

    #[test]
    fn test_fetch_sep24_transaction_status_rejects_http_more_info_url() {
        let raw = RawSep24TransactionResponse {
            id: "tx-789".to_string(),
            status: "completed".to_string(),
            more_info_url: Some("http://anchor.example.com/tx/tx-789".to_string()),
            stellar_transaction_id: None,
        };
        assert!(fetch_sep24_transaction_status(raw).is_err());
    }

    #[test]
    fn test_fetch_sep24_transaction_status_rejects_relative_more_info_url() {
        let raw = RawSep24TransactionResponse {
            id: "tx-789".to_string(),
            status: "completed".to_string(),
            more_info_url: Some("/tx/tx-789".to_string()),
            stellar_transaction_id: None,
        };
        assert!(fetch_sep24_transaction_status(raw).is_err());
    }

    #[test]
    fn test_fetch_sep24_transaction_status_none_more_info_url_ok() {
        let raw = RawSep24TransactionResponse {
            id: "tx-789".to_string(),
            status: "completed".to_string(),
            more_info_url: None,
            stellar_transaction_id: None,
        };
        assert!(fetch_sep24_transaction_status(raw).is_ok());
    }

    #[test]
    fn test_fetch_sep24_transaction_status_missing_id() {
        let raw = RawSep24TransactionResponse {
            id: "".to_string(),
            status: "completed".to_string(),
            more_info_url: None,
            stellar_transaction_id: None,
        };
        assert!(fetch_sep24_transaction_status(raw).is_err());
    }

    #[test]
    fn test_fetch_sep24_transaction_status_missing_status() {
        let raw = RawSep24TransactionResponse {
            id: "tx-789".to_string(),
            status: "".to_string(),
            more_info_url: None,
            stellar_transaction_id: None,
        };
        assert!(fetch_sep24_transaction_status(raw).is_err());
    }

    #[test]
    fn test_fetch_sep24_transaction_status_pending() {
        let raw = RawSep24TransactionResponse {
            id: "tx-pending".to_string(),
            status: "pending_user".to_string(),
            more_info_url: None,
            stellar_transaction_id: None,
        };
        let result = fetch_sep24_transaction_status(raw).unwrap();
        assert_eq!(result.status, TransactionStatus::PendingUser);
    }
}
