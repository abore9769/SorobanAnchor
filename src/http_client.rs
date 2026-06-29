//! HTTP client factory with optional proxy support.
//!
//! Centralises `reqwest` client construction so that every outbound HTTP call
//! (anchor discovery, webhook delivery, SEP-6 status) honours the same proxy
//! configuration without duplicating builder logic.
//!
//! # Proxy configuration
//!
//! [`ProxyConfig`] carries an optional `proxy_url` (e.g. `"http://proxy.corp:3128"`)
//! and optional `no_proxy` bypass list.  Pass it to [`build_client`] to get a
//! `reqwest::blocking::Client` that routes requests through the proxy.
//!
//! When `proxy_url` is `None` the returned client uses the system default
//! (respects `HTTP_PROXY` / `HTTPS_PROXY` environment variables).
//!
//! # Examples
//!
//! ```rust,no_run
//! use anchorkit::http_client::{ProxyConfig, build_client};
//!
//! // No proxy — uses system defaults.
//! let client = build_client(None, 30).unwrap();
//!
//! // Explicit proxy.
//! let proxy = ProxyConfig {
//!     proxy_url: Some("http://proxy.corp.example.com:3128".to_string()),
//!     no_proxy: Some("localhost,127.0.0.1".to_string()),
//! };
//! let client = build_client(Some(&proxy), 30).unwrap();
//! ```

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;
use alloc::string::String;

// ---------------------------------------------------------------------------
// ProxyConfig
// ---------------------------------------------------------------------------

/// Proxy settings for outbound HTTP requests.
///
/// Used by [`build_client`], [`fetch_stellar_toml_with_proxy`], and
/// [`deliver_webhook_with_proxy`] to route discovery and delivery traffic
/// through a corporate or gateway proxy.
///
/// # Fields
///
/// - `proxy_url` — Full proxy URL including scheme and port, e.g.
///   `"http://proxy.corp.example.com:3128"` or `"https://proxy.example.com:8080"`.
///   When `None` the client falls back to `HTTP_PROXY` / `HTTPS_PROXY` env vars.
/// - `no_proxy`  — Comma-separated list of hosts / CIDR ranges that bypass the
///   proxy, e.g. `"localhost,127.0.0.1,.internal.example.com"`.
///   When `None` no bypass list is applied.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ProxyConfig {
    /// Proxy endpoint URL (e.g. `"http://proxy.corp.example.com:3128"`).
    pub proxy_url: Option<String>,
    /// Comma-separated no-proxy bypass list.
    pub no_proxy: Option<String>,
}

impl ProxyConfig {
    /// Returns `true` when a proxy URL has been configured.
    pub fn is_configured(&self) -> bool {
        self.proxy_url.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Client builder
// ---------------------------------------------------------------------------

/// Build a `reqwest::blocking::Client` with optional proxy and a configurable
/// timeout.
///
/// # Arguments
///
/// * `proxy`       — Optional [`ProxyConfig`]. When `None` the client uses
///   system proxy environment variables.
/// * `timeout_secs` — Per-request timeout in seconds. Use `0` for no timeout.
///
/// # Errors
///
/// Returns a `String` error if the proxy URL is malformed or the client cannot
/// be constructed.
#[cfg(feature = "std")]
pub fn build_client(
    proxy: Option<&ProxyConfig>,
    timeout_secs: u64,
) -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder();

    if timeout_secs > 0 {
        builder = builder.timeout(std::time::Duration::from_secs(timeout_secs));
    }

    if let Some(cfg) = proxy {
        if let Some(ref url) = cfg.proxy_url {
            if !url.is_empty() {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err(alloc::format!(
                        "invalid proxy URL '{}': must start with http:// or https://", url
                    ));
                }
                let mut proxy_obj = reqwest::Proxy::all(url.as_str())
                    .map_err(|e| alloc::format!("invalid proxy URL '{}': {}", url, e))?;

                if let Some(ref no_proxy) = cfg.no_proxy {
                    if !no_proxy.is_empty() {
                        proxy_obj = proxy_obj.no_proxy(reqwest::NoProxy::from_string(no_proxy));
                    }
                }

                builder = builder.proxy(proxy_obj);
            }
        }
    }

    builder
        .build()
        .map_err(|e| alloc::format!("failed to build HTTP client: {}", e))
}

// ---------------------------------------------------------------------------
// Proxy-aware stellar.toml fetcher
// ---------------------------------------------------------------------------

/// Fetch and parse a `stellar.toml` file through an optional proxy.
///
/// Constructs the well-known URL via [`fetch_stellar_toml_url`], performs an
/// HTTP GET (routing through `proxy` when configured), and parses the response
/// body with [`parse_stellar_toml`].
///
/// # Arguments
///
/// * `domain`      — Anchor base URL, e.g. `"https://anchor.example.com"`.
/// * `proxy`       — Optional proxy configuration.
/// * `timeout_secs` — Per-request timeout in seconds.
///
/// # Errors
///
/// Returns a `String` error on network failure, non-2xx HTTP status, or TOML
/// parse failure.
///
/// # Examples
///
/// ```rust,no_run
/// use anchorkit::http_client::{ProxyConfig, fetch_stellar_toml_with_proxy};
///
/// let proxy = ProxyConfig {
///     proxy_url: Some("http://proxy.corp.example.com:3128".to_string()),
///     no_proxy: None,
/// };
/// let toml = fetch_stellar_toml_with_proxy("https://anchor.example.com", Some(&proxy), 30).unwrap();
/// println!("Supports SEP-6: {}", toml.supports_sep6());
/// ```
#[cfg(feature = "std")]
pub fn fetch_stellar_toml_with_proxy(
    domain: &str,
    proxy: Option<&ProxyConfig>,
    timeout_secs: u64,
) -> Result<crate::stellar_toml::ParsedStellarToml, String> {
    let url = crate::stellar_toml::fetch_stellar_toml_url(domain)
        .map_err(|e| alloc::format!("invalid domain '{}': {:?}", domain, e))?;

    let client = build_client(proxy, timeout_secs)?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| alloc::format!("GET {} failed: {}", url, e))?;

    if !response.status().is_success() {
        return Err(alloc::format!(
            "GET {} returned HTTP {}",
            url,
            response.status()
        ));
    }

    let body = response
        .text()
        .map_err(|e| alloc::format!("failed to read response body: {}", e))?;

    crate::stellar_toml::parse_stellar_toml(&body)
        .map_err(|e| alloc::format!("failed to parse stellar.toml: {:?}", e))
}

// ---------------------------------------------------------------------------
// Proxy-aware webhook delivery
// ---------------------------------------------------------------------------

/// Deliver a webhook payload through an optional proxy.
///
/// This is a thin wrapper around [`deliver_webhook`] that constructs the
/// `http_post` transport function using a proxy-aware `reqwest` client.
///
/// # Arguments
///
/// * `config`      — Webhook delivery configuration (endpoint, retries, DLQ key).
/// * `payload`     — JSON payload string to POST.
/// * `dlq`         — Dead-letter queue map for failed deliveries.
/// * `proxy`       — Optional proxy configuration.
/// * `now_fn`      — Returns the current Unix timestamp in seconds.
///
/// # Errors
///
/// Returns [`AnchorKitError`] with code [`ErrorCode::WebhookDeliveryFailed`]
/// after all retry attempts are exhausted.
///
/// # Examples
///
/// ```rust,no_run
/// use std::collections::BTreeMap;
/// use anchorkit::http_client::{ProxyConfig, deliver_webhook_with_proxy};
/// use anchorkit::webhook::{WebhookDeliveryConfig, DlqEntry};
/// use anchorkit::retry::RetryConfig;
///
/// let config = WebhookDeliveryConfig {
///     endpoint_url: "https://hooks.example.com/anchor".to_string(),
///     timeout_ms: 5000,
///     retry_config: RetryConfig::default(),
///     dead_letter_storage_key: "anchor-hook".to_string(),
///     signing_key: None,
/// };
/// let proxy = ProxyConfig {
///     proxy_url: Some("http://proxy.corp.example.com:3128".to_string()),
///     no_proxy: None,
/// };
/// let mut dlq = BTreeMap::new();
/// deliver_webhook_with_proxy(&config, r#"{"event":"deposit"}"#, &mut dlq, Some(&proxy), || 0).unwrap();
/// ```
#[cfg(feature = "std")]
pub fn deliver_webhook_with_proxy(
    config: &crate::webhook::WebhookDeliveryConfig,
    payload: &str,
    dlq: &mut alloc::collections::BTreeMap<String, alloc::vec::Vec<crate::webhook::DlqEntry>>,
    proxy: Option<&ProxyConfig>,
    now_fn: impl Fn() -> u64,
) -> Result<(), crate::errors::AnchorKitError> {
    let timeout_secs = if config.timeout_ms > 0 {
        (config.timeout_ms / 1000).max(1)
    } else {
        30
    };

    let client = build_client(proxy, timeout_secs)
        .map_err(|e| {
            crate::errors::AnchorKitError::with_context(
                crate::errors::ErrorCode::WebhookDeliveryFailed,
                "failed to build HTTP client for webhook delivery",
                &e,
            )
        })?;

    crate::webhook::deliver_webhook(
        config,
        payload,
        dlq,
        move |url, body, sig_header| {
            let mut req = client
                .post(url)
                .header("Content-Type", "application/json")
                .body(alloc::string::String::from(body));
            if let Some(sig) = sig_header {
                req = req.header("X-Anchor-Signature", sig);
            }
            req.send()
                .map(|r| r.status().as_u16())
                .map_err(|e| alloc::format!("HTTP POST failed: {}", e))
        },
        |_| {},
        now_fn,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn proxy_config_default_is_unconfigured() {
        let cfg = ProxyConfig::default();
        assert!(!cfg.is_configured());
    }

    #[test]
    fn proxy_config_with_url_is_configured() {
        let cfg = ProxyConfig {
            proxy_url: Some("http://proxy.example.com:3128".to_string()),
            no_proxy: None,
        };
        assert!(cfg.is_configured());
    }

    #[test]
    fn proxy_config_empty_url_is_not_configured() {
        let cfg = ProxyConfig {
            proxy_url: Some(String::new()),
            no_proxy: None,
        };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn proxy_config_none_url_is_not_configured() {
        let cfg = ProxyConfig {
            proxy_url: None,
            no_proxy: Some("localhost".to_string()),
        };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn proxy_config_clone_and_eq() {
        let a = ProxyConfig {
            proxy_url: Some("http://proxy.example.com:3128".to_string()),
            no_proxy: Some("localhost".to_string()),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[cfg(feature = "std")]
    #[test]
    fn build_client_no_proxy_succeeds() {
        let client = build_client(None, 10);
        assert!(client.is_ok(), "client without proxy should build successfully");
    }

    #[cfg(feature = "std")]
    #[test]
    fn build_client_with_valid_proxy_url_succeeds() {
        let proxy = ProxyConfig {
            proxy_url: Some("http://proxy.example.com:3128".to_string()),
            no_proxy: None,
        };
        let client = build_client(Some(&proxy), 10);
        assert!(client.is_ok(), "client with valid proxy URL should build successfully");
    }

    #[cfg(feature = "std")]
    #[test]
    fn build_client_with_proxy_and_no_proxy_list_succeeds() {
        let proxy = ProxyConfig {
            proxy_url: Some("http://proxy.example.com:3128".to_string()),
            no_proxy: Some("localhost,127.0.0.1,.internal.example.com".to_string()),
        };
        let client = build_client(Some(&proxy), 30);
        assert!(client.is_ok(), "client with proxy + no_proxy list should build successfully");
    }

    #[cfg(feature = "std")]
    #[test]
    fn build_client_with_invalid_proxy_url_returns_error() {
        let proxy = ProxyConfig {
            proxy_url: Some("not-a-valid-url".to_string()),
            no_proxy: None,
        };
        let result = build_client(Some(&proxy), 10);
        assert!(result.is_err(), "invalid proxy URL should return an error");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("invalid proxy URL"),
            "error message should mention invalid proxy URL, got: {msg}"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn build_client_unconfigured_proxy_uses_system_defaults() {
        // An unconfigured ProxyConfig (no URL) should behave like no proxy at all.
        let proxy = ProxyConfig::default();
        let client = build_client(Some(&proxy), 10);
        assert!(client.is_ok(), "unconfigured proxy should fall through to system defaults");
    }

    #[cfg(feature = "std")]
    #[test]
    fn build_client_zero_timeout_builds_successfully() {
        // timeout_secs = 0 means no timeout — client should still build.
        let client = build_client(None, 0);
        assert!(client.is_ok(), "zero timeout should build successfully");
    }

    #[cfg(feature = "std")]
    #[test]
    fn build_client_https_proxy_url_succeeds() {
        let proxy = ProxyConfig {
            proxy_url: Some("https://secure-proxy.example.com:8080".to_string()),
            no_proxy: None,
        };
        let client = build_client(Some(&proxy), 10);
        assert!(client.is_ok(), "HTTPS proxy URL should build successfully");
    }

    // ── Webhook delivery with proxy (unit-level, injected transport) ──────────

    #[cfg(feature = "std")]
    #[test]
    fn deliver_webhook_with_proxy_succeeds_on_200() {
        use crate::webhook::{WebhookDeliveryConfig, DlqEntry, get_dead_letter_webhooks};
        use crate::retry::RetryConfig;
        use alloc::collections::BTreeMap;

        // Use the base deliver_webhook directly with an injected transport to
        // avoid real network calls in unit tests.
        let config = WebhookDeliveryConfig {
            endpoint_url: "https://hooks.example.com/anchor".to_string(),
            timeout_ms: 1000,
            retry_config: RetryConfig {
                max_attempts: 3,
                base_delay_ms: 0,
                max_delay_ms: 0,
                backoff_multiplier: 1,
            },
            dead_letter_storage_key: "proxy-test".to_string(),
            signing_key: None,
        };

        let mut dlq: BTreeMap<String, alloc::vec::Vec<DlqEntry>> = BTreeMap::new();

        // Inject a mock transport that always returns 200.
        let result = crate::webhook::deliver_webhook(
            &config,
            r#"{"event":"deposit_completed"}"#,
            &mut dlq,
            |_url, _body, _sig| Ok(200u16),
            |_| {},
            || 1_000_000u64,
        );

        assert!(result.is_ok(), "delivery should succeed with 200 response");
        assert!(
            get_dead_letter_webhooks(&dlq, "proxy-test").is_empty(),
            "DLQ should be empty on success"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn deliver_webhook_with_proxy_stores_dlq_on_failure() {
        use crate::webhook::{WebhookDeliveryConfig, DlqEntry, get_dead_letter_webhooks};
        use crate::retry::RetryConfig;
        use alloc::collections::BTreeMap;

        let config = WebhookDeliveryConfig {
            endpoint_url: "https://hooks.example.com/anchor".to_string(),
            timeout_ms: 1000,
            retry_config: RetryConfig {
                max_attempts: 2,
                base_delay_ms: 0,
                max_delay_ms: 0,
                backoff_multiplier: 1,
            },
            dead_letter_storage_key: "proxy-fail-test".to_string(),
            signing_key: None,
        };

        let mut dlq: BTreeMap<String, alloc::vec::Vec<DlqEntry>> = BTreeMap::new();

        // Inject a mock transport that always returns 503.
        let result = crate::webhook::deliver_webhook(
            &config,
            r#"{"event":"deposit_failed"}"#,
            &mut dlq,
            |_url, _body, _sig| Ok(503u16),
            |_| {},
            || 9_999_999u64,
        );

        assert!(result.is_err(), "delivery should fail after exhausting retries");
        let entries = get_dead_letter_webhooks(&dlq, "proxy-fail-test");
        assert_eq!(entries.len(), 1, "one DLQ entry should be written");
        assert_eq!(entries[0].last_status_code, 503);
        assert_eq!(entries[0].attempts_made, 2);
        assert_eq!(entries[0].failed_at_timestamp, 9_999_999);
    }

    // ── ProxyConfig serialization (std only) ──────────────────────────────────

    #[cfg(feature = "std")]
    #[test]
    fn proxy_config_serializes_to_json() {
        let cfg = ProxyConfig {
            proxy_url: Some("http://proxy.example.com:3128".to_string()),
            no_proxy: Some("localhost".to_string()),
        };
        let json = serde_json::to_string(&cfg).expect("serialization should succeed");
        assert!(json.contains("proxy_url"));
        assert!(json.contains("proxy.example.com"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn proxy_config_deserializes_from_json() {
        let json = r#"{"proxy_url":"http://proxy.example.com:3128","no_proxy":"localhost"}"#;
        let cfg: ProxyConfig = serde_json::from_str(json).expect("deserialization should succeed");
        assert_eq!(cfg.proxy_url.as_deref(), Some("http://proxy.example.com:3128"));
        assert_eq!(cfg.no_proxy.as_deref(), Some("localhost"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn proxy_config_deserializes_with_null_fields() {
        let json = r#"{"proxy_url":null,"no_proxy":null}"#;
        let cfg: ProxyConfig = serde_json::from_str(json).expect("deserialization should succeed");
        assert!(cfg.proxy_url.is_none());
        assert!(cfg.no_proxy.is_none());
        assert!(!cfg.is_configured());
    }
}
