//! Webhook delivery with exponential backoff and a Dead Letter Queue (DLQ).
//!
//! `deliver_webhook` wraps the HTTP POST in `retry_with_backoff`.  On total
//! exhaustion the serialised payload is written into the caller-supplied DLQ
//! map under `dead_letter_storage_key`.  `get_dead_letter_webhooks` lets
//! admins inspect those failed entries.

extern crate alloc;

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::RefCell;

use crate::{
    errors::{AnchorKitError, ErrorCode},
    retry::{retry_with_backoff, RetryConfig},
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for a single webhook endpoint.
#[derive(Clone, Debug)]
pub struct WebhookDeliveryConfig {
    /// Target URL for the HTTP POST.
    pub endpoint_url: String,
    /// Maximum delivery attempts (passed straight to `RetryConfig::max_attempts`).
    pub max_retries: u32,
    /// Backoff parameters (base delay, multiplier, cap).
    pub retry_config: RetryConfig,
    /// Key under which a failed payload is stored in the DLQ map.
    pub dead_letter_storage_key: String,
}

// ---------------------------------------------------------------------------
// Delivery
// ---------------------------------------------------------------------------

/// Attempt to POST `payload` to `config.endpoint_url` with exponential backoff.
///
/// `http_post` is an injectable transport function `(url, body) -> Result<u16, String>`
/// that returns the HTTP status code on success or an error string on failure.
/// This keeps the core logic testable without a live network.
///
/// `sleep_fn` is called with the computed delay (ms) between retries.
///
/// On total failure the payload is written to `dlq` under
/// `config.dead_letter_storage_key` and an `AnchorKitError` is returned.
pub fn deliver_webhook<H, S>(
    config: &WebhookDeliveryConfig,
    payload: &str,
    dlq: &mut BTreeMap<String, Vec<String>>,
    http_post: H,
    mut sleep_fn: S,
) -> Result<(), AnchorKitError>
where
    H: Fn(&str, &str) -> Result<u16, String>,
    S: FnMut(u64),
{
    let mut retry_cfg = config.retry_config.clone();
    retry_cfg.max_attempts = config.max_retries;

    // RefCell lets the closure capture last_error_msg mutably while the
    // closure itself is only FnMut (not FnOnce).
    let last_error_msg: RefCell<String> = RefCell::new(String::new());

    let result = retry_with_backoff(
        &retry_cfg,
        |attempt| {
            let msg = match http_post(&config.endpoint_url, payload) {
                Ok(s) if s < 400 => return Ok(()),
                Ok(s) => format!("HTTP {s}"),
                Err(e) => e,
            };
            let delay = retry_cfg.delay_for_attempt(attempt, attempt as u64 * 17 + 3);
            #[cfg(feature = "std")]
            eprintln!(
                "[webhook] attempt={} error=\"{}\" next_delay_ms={}",
                attempt + 1,
                msg,
                delay
            );
            *last_error_msg.borrow_mut() = msg.clone();
            Err(msg)
        },
        |_e: &String| true, // all HTTP/transport errors are retryable
        &mut sleep_fn,
    );

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            dlq.entry(config.dead_letter_storage_key.clone())
                .or_default()
                .push(payload.to_string());

            let attempts_made = config.max_retries;
            let last = last_error_msg.into_inner();
            Err(AnchorKitError::with_context(
                ErrorCode::WebhookDeliveryFailed,
                &format!(
                    "Webhook delivery failed after {} attempt(s): {}",
                    attempts_made, e
                ),
                &format!("attempts_made={} last_error={}", attempts_made, last),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// DLQ inspection
// ---------------------------------------------------------------------------

/// Return all payloads stored under `key` in the DLQ, or an empty slice.
pub fn get_dead_letter_webhooks<'a>(
    dlq: &'a BTreeMap<String, Vec<String>>,
    key: &str,
) -> &'a [String] {
    dlq.get(key).map(Vec::as_slice).unwrap_or(&[])
}
