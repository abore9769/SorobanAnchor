//! Minimal stellar.toml capability parser.
//!
//! Parses the key=value fields relevant to anchor capability discovery
//! (SEP-6, SEP-24, SEP-10, KYC) from a raw stellar.toml string.
//! No external TOML crate is required; only `alloc` is used.

#![cfg_attr(not(test), no_std)]

extern crate alloc;
use alloc::{string::String, vec::Vec};

use crate::domain_validator::validate_anchor_domain;
use crate::errors::AnchorKitError;

/// Parsed representation of the anchor-relevant fields in a stellar.toml file.
/// All URL fields are validated with [`validate_anchor_domain`] before being stored.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedStellarToml {
    pub transfer_server: Option<String>,
    pub transfer_server_sep0024: Option<String>,
    pub kyc_server: Option<String>,
    pub web_auth_endpoint: Option<String>,
    pub signing_key: Option<String>,
    /// Asset codes declared in [[CURRENCIES]] sections.
    pub supported_assets: Vec<String>,
}

impl ParsedStellarToml {
    /// Returns `true` if the anchor declares SEP-6 support.
    pub fn supports_sep6(&self) -> bool {
        self.transfer_server.is_some()
    }

    /// Returns `true` if the anchor declares SEP-24 support.
    pub fn supports_sep24(&self) -> bool {
        self.transfer_server_sep0024.is_some()
    }

    /// Returns `true` if the anchor declares SEP-10 (web auth) support.
    pub fn supports_sep10(&self) -> bool {
        self.web_auth_endpoint.is_some()
    }
}

/// Constructs the well-known stellar.toml URL for a given domain.
///
/// # Errors
/// Returns `Err` if `domain` fails [`validate_anchor_domain`].
pub fn fetch_stellar_toml_url(domain: &str) -> Result<String, AnchorKitError> {
    validate_anchor_domain(domain)?;
    let mut url = String::from(domain);
    // Strip trailing slash before appending path
    if url.ends_with('/') {
        url.pop();
    }
    url.push_str("/.well-known/stellar.toml");
    Ok(url)
}

/// Parse a raw stellar.toml string into a [`ParsedStellarToml`].
///
/// Only top-level key = "value" assignments and `[[CURRENCIES]]` `code` fields
/// are extracted. All URL fields are validated; an invalid URL causes an error.
///
/// # Errors
/// Returns `Err` if any URL field contains an invalid value.
pub fn parse_stellar_toml(raw: &str) -> Result<ParsedStellarToml, AnchorKitError> {
    let mut transfer_server: Option<String> = None;
    let mut transfer_server_sep0024: Option<String> = None;
    let mut kyc_server: Option<String> = None;
    let mut web_auth_endpoint: Option<String> = None;
    let mut signing_key: Option<String> = None;
    let mut supported_assets: Vec<String> = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        // Skip comments and section headers (except [[CURRENCIES]])
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some((key, value)) = parse_kv(line) {
            match key {
                "TRANSFER_SERVER" => {
                    validate_anchor_domain(&value)?;
                    transfer_server = Some(value);
                }
                "TRANSFER_SERVER_SEP0024" => {
                    validate_anchor_domain(&value)?;
                    transfer_server_sep0024 = Some(value);
                }
                "KYC_SERVER" => {
                    validate_anchor_domain(&value)?;
                    kyc_server = Some(value);
                }
                "WEB_AUTH_ENDPOINT" => {
                    validate_anchor_domain(&value)?;
                    web_auth_endpoint = Some(value);
                }
                "SIGNING_KEY" => {
                    signing_key = Some(value);
                }
                "code" => {
                    // Inside a [[CURRENCIES]] table
                    if !value.is_empty() && !supported_assets.contains(&value) {
                        supported_assets.push(value);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(ParsedStellarToml {
        transfer_server,
        transfer_server_sep0024,
        kyc_server,
        web_auth_endpoint,
        signing_key,
        supported_assets,
    })
}

/// Extract (key, value) from a line of the form `KEY = "value"` or `KEY = value`.
/// Returns `None` if the line is not a key=value assignment.
fn parse_kv(line: &str) -> Option<(&str, String)> {
    let eq = line.find('=')?;
    let key = line[..eq].trim();
    let raw_val = line[eq + 1..].trim();
    // Strip surrounding quotes if present
    let value = if raw_val.starts_with('"') && raw_val.ends_with('"') && raw_val.len() >= 2 {
        &raw_val[1..raw_val.len() - 1]
    } else {
        raw_val
    };
    // Skip inline comments after the value
    let value = value.split('#').next().unwrap_or(value).trim();
    if key.is_empty() {
        return None;
    }
    Some((key, String::from(value)))
}
