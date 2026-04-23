//! SEP-24 Interactive Deposit & Withdrawal Service Layer
//!
//! Provides normalized service functions for initiating interactive deposits,
//! interactive withdrawals, and fetching transaction status for SEP-24 flows.

#![cfg_attr(not(test), no_std)]

extern crate alloc;
use alloc::string::{String, ToString};

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
    if raw.url.is_empty() {
        return Err(AnchorKitError::new(
            ErrorCode::ValidationError,
            "Missing url field in interactive deposit response",
        ));
    }
    if raw.id.is_empty() {
        return Err(AnchorKitError::new(
            ErrorCode::ValidationError,
            "Missing id field in interactive deposit response",
        ));
    }

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
    if raw.url.is_empty() {
        return Err(AnchorKitError::new(
            ErrorCode::ValidationError,
            "Missing url field in interactive withdrawal response",
        ));
    }
    if raw.id.is_empty() {
        return Err(AnchorKitError::new(
            ErrorCode::ValidationError,
            "Missing id field in interactive withdrawal response",
        ));
    }

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
