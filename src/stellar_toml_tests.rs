#![cfg(test)]

use crate::stellar_toml::{fetch_stellar_toml_url, parse_stellar_toml};

const VALID_TOML: &str = r#"
NETWORK_PASSPHRASE = "Test SDF Network ; September 2015"
TRANSFER_SERVER = "https://api.example.com"
TRANSFER_SERVER_SEP0024 = "https://api.example.com/sep24"
KYC_SERVER = "https://kyc.example.com"
WEB_AUTH_ENDPOINT = "https://auth.example.com"
SIGNING_KEY = "GSIGN123"

[[CURRENCIES]]
code = "USDC"
issuer = "GABC123"

[[CURRENCIES]]
code = "XLM"
issuer = "native"
"#;

#[test]
fn test_parse_valid_toml_extracts_all_fields() {
    let parsed = parse_stellar_toml(VALID_TOML).unwrap();
    assert_eq!(parsed.transfer_server.as_deref(), Some("https://api.example.com"));
    assert_eq!(parsed.transfer_server_sep0024.as_deref(), Some("https://api.example.com/sep24"));
    assert_eq!(parsed.kyc_server.as_deref(), Some("https://kyc.example.com"));
    assert_eq!(parsed.web_auth_endpoint.as_deref(), Some("https://auth.example.com"));
    assert_eq!(parsed.signing_key.as_deref(), Some("GSIGN123"));
    assert_eq!(parsed.supported_assets, vec!["USDC", "XLM"]);
}

#[test]
fn test_parse_sep_support_flags() {
    let parsed = parse_stellar_toml(VALID_TOML).unwrap();
    assert!(parsed.supports_sep6());
    assert!(parsed.supports_sep24());
    assert!(parsed.supports_sep10());
}

#[test]
fn test_parse_missing_optional_fields_returns_none() {
    let raw = r#"SIGNING_KEY = "GSIGN123""#;
    let parsed = parse_stellar_toml(raw).unwrap();
    assert!(parsed.transfer_server.is_none());
    assert!(parsed.transfer_server_sep0024.is_none());
    assert!(parsed.kyc_server.is_none());
    assert!(parsed.web_auth_endpoint.is_none());
    assert!(parsed.supported_assets.is_empty());
    assert!(!parsed.supports_sep6());
    assert!(!parsed.supports_sep24());
    assert!(!parsed.supports_sep10());
}

#[test]
fn test_parse_empty_toml_returns_empty_capabilities() {
    let parsed = parse_stellar_toml("").unwrap();
    assert!(parsed.transfer_server.is_none());
    assert!(parsed.supported_assets.is_empty());
}

#[test]
fn test_parse_invalid_url_in_transfer_server_rejected() {
    let raw = r#"TRANSFER_SERVER = "http://insecure.example.com""#;
    assert!(parse_stellar_toml(raw).is_err());
}

#[test]
fn test_parse_invalid_url_in_web_auth_endpoint_rejected() {
    let raw = r#"WEB_AUTH_ENDPOINT = "not-a-url""#;
    assert!(parse_stellar_toml(raw).is_err());
}

#[test]
fn test_parse_invalid_url_in_kyc_server_rejected() {
    let raw = r#"KYC_SERVER = "ftp://kyc.example.com""#;
    assert!(parse_stellar_toml(raw).is_err());
}

#[test]
fn test_parse_comments_and_blank_lines_ignored() {
    let raw = r#"
# This is a comment
TRANSFER_SERVER = "https://api.example.com"

# Another comment
SIGNING_KEY = "GSIGN123"
"#;
    let parsed = parse_stellar_toml(raw).unwrap();
    assert_eq!(parsed.transfer_server.as_deref(), Some("https://api.example.com"));
    assert_eq!(parsed.signing_key.as_deref(), Some("GSIGN123"));
}

#[test]
fn test_parse_duplicate_currency_codes_deduplicated() {
    let raw = r#"
[[CURRENCIES]]
code = "USDC"

[[CURRENCIES]]
code = "USDC"
"#;
    let parsed = parse_stellar_toml(raw).unwrap();
    assert_eq!(parsed.supported_assets.len(), 1);
}

#[test]
fn test_fetch_stellar_toml_url_valid_domain() {
    let url = fetch_stellar_toml_url("https://example.com").unwrap();
    assert_eq!(url, "https://example.com/.well-known/stellar.toml");
}

#[test]
fn test_fetch_stellar_toml_url_strips_trailing_slash() {
    let url = fetch_stellar_toml_url("https://example.com/").unwrap();
    assert_eq!(url, "https://example.com/.well-known/stellar.toml");
}

#[test]
fn test_fetch_stellar_toml_url_rejects_http() {
    assert!(fetch_stellar_toml_url("http://example.com").is_err());
}

#[test]
fn test_fetch_stellar_toml_url_rejects_invalid_domain() {
    assert!(fetch_stellar_toml_url("not-a-domain").is_err());
}
