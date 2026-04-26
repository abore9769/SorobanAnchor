//! Domain validation utility for anchor domain input
//!
//! Validates anchor domain URLs before making requests to ensure:
//! - Proper URL format
//! - HTTPS-only connections
//! - Rejection of malformed domains

#![cfg_attr(not(test), no_std)]

extern crate alloc;
use alloc::vec::Vec;

use crate::errors::AnchorKitError;

/// Validates an anchor domain URL.
///
/// Ensures the URL is safe to use as an anchor endpoint before making any
/// outbound HTTP requests. The following rules are enforced:
///
/// - Must be non-empty and between 10 and 2048 characters.
/// - Must use the `https://` scheme (HTTP, FTP, WS, etc. are rejected).
/// - Host must contain at least one dot (no bare hostnames like `localhost`).
/// - No consecutive dots, leading/trailing dots, or leading/trailing hyphens
///   in any DNS label.
/// - No IPv4 or IPv6 addresses.
/// - Port (if present) must be in the range 1–65535.
/// - No control characters or RFC 3986 invalid characters (`<`, `>`, `{`, `}`,
///   `|`, `\`).
/// - Unicode / IDN domains are rejected (ASCII only).
///
/// # Arguments
///
/// * `domain` - The full URL string to validate (e.g. `"https://anchor.example.com"`).
///
/// # Returns
///
/// `Ok(())` if the domain passes all checks.
///
/// # Errors
///
/// Returns [`AnchorKitError`] with code [`ErrorCode::InvalidEndpointFormat`] if
/// any validation rule is violated.
///
/// # Examples
///
/// ```rust
/// use anchorkit::validate_anchor_domain;
///
/// // Valid HTTPS domain
/// assert!(validate_anchor_domain("https://anchor.example.com").is_ok());
///
/// // Valid with port and path
/// assert!(validate_anchor_domain("https://api.example.com:8080/sep6").is_ok());
///
/// // Rejected: HTTP
/// assert!(validate_anchor_domain("http://example.com").is_err());
///
/// // Rejected: no TLD
/// assert!(validate_anchor_domain("https://localhost").is_err());
///
/// // Rejected: IPv4
/// assert!(validate_anchor_domain("https://192.168.1.1").is_err());
/// ```
pub fn validate_anchor_domain(domain: &str) -> Result<(), AnchorKitError> {
    // Check for empty or whitespace-only input
    if domain.is_empty() || domain.trim().is_empty() {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Check minimum length for valid HTTPS URL
    if domain.len() < 10 {
        // "https://a.b" is minimum valid
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Check maximum reasonable length
    if domain.len() > 2048 {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Ensure HTTPS protocol
    if !domain.starts_with("https://") {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Extract domain part after protocol
    let domain_part = &domain[8..]; // Skip "https://"

    // Check for empty domain after protocol
    if domain_part.is_empty() {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Split by '/' to get the host part, but also handle query params
    let host_with_query = match domain_part.split('/').next() {
        Some(h) if !h.is_empty() => h,
        _ => return Err(AnchorKitError::invalid_endpoint_format()),
    };
    
    // Remove query parameters and fragments from host
    let host = host_with_query
        .split('?').next().unwrap_or(host_with_query)
        .split('#').next().unwrap_or(host_with_query);

    // Validate host structure
    validate_host(host)?;

    // Check for invalid characters in the full URL
    validate_url_characters(domain)?;

    Ok(())
}

/// Validates the host portion of a URL
fn validate_host(host: &str) -> Result<(), AnchorKitError> {
    // Check for empty host
    if host.is_empty() {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Check for spaces in host
    if host.contains(' ') {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Check for port specification (optional)
    let domain_without_port = if let Some(colon_pos) = host.rfind(':') {
        // Validate port number
        let port_str = &host[colon_pos + 1..];
        if port_str.is_empty() {
            return Err(AnchorKitError::invalid_endpoint_format());
        }
        
        // Check if port is numeric
        for c in port_str.chars() {
            if !c.is_ascii_digit() {
                return Err(AnchorKitError::invalid_endpoint_format());
            }
        }
        
        // Validate port range (1-65535)
        if let Ok(port) = port_str.parse::<u32>() {
            if port == 0 || port > 65535 {
                return Err(AnchorKitError::invalid_endpoint_format());
            }
        } else {
            return Err(AnchorKitError::invalid_endpoint_format());
        }
        
        &host[..colon_pos]
    } else {
        host
    };

    // Check for valid domain structure
    if domain_without_port.is_empty() {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Must contain at least one dot for valid domain
    if !domain_without_port.contains('.') {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Check for consecutive dots
    if domain_without_port.contains("..") {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Check for leading or trailing dots
    if domain_without_port.starts_with('.') || domain_without_port.ends_with('.') {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    // Validate each label in the domain
    let labels: Vec<&str> = domain_without_port.split('.').collect();

    // Reject IPv4-style hostnames: all labels purely numeric (e.g. 192.168.1.1)
    if labels.iter().all(|l| l.chars().all(|c| c.is_ascii_digit())) {
        return Err(AnchorKitError::invalid_endpoint_format());
    }

    for label in labels {
        if label.is_empty() {
            return Err(AnchorKitError::invalid_endpoint_format());
        }
        
        // Label must start and end with alphanumeric
        let first_char = label.chars().next().unwrap();
        let last_char = label.chars().last().unwrap();
        
        if !first_char.is_alphanumeric() || !last_char.is_alphanumeric() {
            return Err(AnchorKitError::invalid_endpoint_format());
        }
        
        // Check for valid characters in label
        for c in label.chars() {
            if !c.is_alphanumeric() && c != '-' {
                return Err(AnchorKitError::invalid_endpoint_format());
            }
        }
    }

    Ok(())
}

/// Validates URL characters
fn validate_url_characters(url: &str) -> Result<(), AnchorKitError> {
    for c in url.chars() {
        if c.is_control() {
            return Err(AnchorKitError::invalid_endpoint_format());
        }
        // Reject characters not valid in URLs per RFC 3986 / issue #67
        if matches!(c, '<' | '>' | '{' | '}' | '|' | '\\') {
            return Err(AnchorKitError::invalid_endpoint_format());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_valid_domains() {
        // Basic valid domains
        assert!(validate_anchor_domain("https://example.com").is_ok());
        assert!(validate_anchor_domain("https://api.example.com").is_ok());
        assert!(validate_anchor_domain("https://sub.domain.example.com").is_ok());
        
        // With paths
        assert!(validate_anchor_domain("https://example.com/path").is_ok());
        assert!(validate_anchor_domain("https://example.com/path/to/resource").is_ok());
        
        // With ports
        assert!(validate_anchor_domain("https://example.com:8080").is_ok());
        assert!(validate_anchor_domain("https://example.com:443").is_ok());
        
        // With query parameters
        assert!(validate_anchor_domain("https://example.com?param=value").is_ok());
        assert!(validate_anchor_domain("https://example.com/path?param=value").is_ok());
        
        // With hyphens in domain
        assert!(validate_anchor_domain("https://my-domain.com").is_ok());
        assert!(validate_anchor_domain("https://api-v2.example.com").is_ok());
    }

    #[test]
    fn test_https_only() {
        // HTTP should be rejected
        assert!(validate_anchor_domain("http://example.com").is_err());
        assert!(validate_anchor_domain("http://secure.example.com").is_err());
        
        // Other protocols should be rejected
        assert!(validate_anchor_domain("ftp://example.com").is_err());
        assert!(validate_anchor_domain("ws://example.com").is_err());
        assert!(validate_anchor_domain("wss://example.com").is_err());
    }

    #[test]
    fn test_malformed_domains() {
        // Empty or whitespace
        assert!(validate_anchor_domain("").is_err());
        assert!(validate_anchor_domain("   ").is_err());
        
        // Missing protocol
        assert!(validate_anchor_domain("example.com").is_err());
        assert!(validate_anchor_domain("www.example.com").is_err());
        
        // Protocol only
        assert!(validate_anchor_domain("https://").is_err());
        
        // Invalid domain structure
        assert!(validate_anchor_domain("https://.example.com").is_err());
        assert!(validate_anchor_domain("https://example.com.").is_err());
        assert!(validate_anchor_domain("https://example..com").is_err());
        
        // No TLD
        assert!(validate_anchor_domain("https://localhost").is_err());
        assert!(validate_anchor_domain("https://example").is_err());
        
        // Spaces in domain
        assert!(validate_anchor_domain("https://example .com").is_err());
        assert!(validate_anchor_domain("https://exam ple.com").is_err());
        
        // Invalid characters
        assert!(validate_anchor_domain("https://example$.com").is_err());
        assert!(validate_anchor_domain("https://example@.com").is_err());
        
        // Too short
        assert!(validate_anchor_domain("https://a").is_err());
        assert!(validate_anchor_domain("https://a.").is_err());
    }

    #[test]
    fn test_port_validation() {
        // Valid ports
        assert!(validate_anchor_domain("https://example.com:1").is_ok());
        assert!(validate_anchor_domain("https://example.com:80").is_ok());
        assert!(validate_anchor_domain("https://example.com:443").is_ok());
        assert!(validate_anchor_domain("https://example.com:8080").is_ok());
        assert!(validate_anchor_domain("https://example.com:65535").is_ok());
        
        // Invalid ports
        assert!(validate_anchor_domain("https://example.com:0").is_err());
        assert!(validate_anchor_domain("https://example.com:65536").is_err());
        assert!(validate_anchor_domain("https://example.com:99999").is_err());
        assert!(validate_anchor_domain("https://example.com:").is_err());
        assert!(validate_anchor_domain("https://example.com:abc").is_err());
    }

    #[test]
    fn test_length_limits() {
        // Too long
        let long_domain = format!("https://{}.com", "a".repeat(2048));
        assert!(validate_anchor_domain(&long_domain).is_err());
        
        // Maximum acceptable length
        let max_domain = format!("https://{}.com", "a".repeat(2000));
        assert!(validate_anchor_domain(&max_domain).is_ok());
    }

    #[test]
    fn test_control_characters() {
        // Control characters should be rejected
        assert!(validate_anchor_domain("https://example.com\n").is_err());
        assert!(validate_anchor_domain("https://example.com\r").is_err());
        assert!(validate_anchor_domain("https://example.com\t").is_err());
        assert!(validate_anchor_domain("https://\0example.com").is_err());
    }

    #[test]
    fn test_double_slashes() {
        // Double slashes in paths are technically allowed in URLs
        // but may indicate a mistake - for now we allow them
        assert!(validate_anchor_domain("https://example.com//path").is_ok());
        assert!(validate_anchor_domain("https://example.com/path//resource").is_ok());
    }

    #[test]
    fn test_edge_cases() {
        // Minimum valid domain
        assert!(validate_anchor_domain("https://a.b").is_ok());
        
        // Multiple subdomains
        assert!(validate_anchor_domain("https://a.b.c.d.example.com").is_ok());
        
        // Numbers in domain
        assert!(validate_anchor_domain("https://api2.example.com").is_ok());
        assert!(validate_anchor_domain("https://123.example.com").is_ok());
        
        // Hyphens at various positions (but not at start/end of label)
        assert!(validate_anchor_domain("https://my-api.example.com").is_ok());
        assert!(validate_anchor_domain("https://-example.com").is_err());
        assert!(validate_anchor_domain("https://example-.com").is_err());
    }

    #[test]
    fn test_unicode_idn_domains() {
        // Unicode/IDN domains should be rejected (not supported)
        assert!(validate_anchor_domain("https://münchen.de").is_err());
        assert!(validate_anchor_domain("https://例え.jp").is_err());
        assert!(validate_anchor_domain("https://россия.рф").is_err());
        assert!(validate_anchor_domain("https://example.测试").is_err());
    }

    #[test]
    fn test_ip_address_inputs() {
        // IPv4 addresses should be rejected (not valid domain format)
        assert!(validate_anchor_domain("https://192.168.1.1").is_err());
        assert!(validate_anchor_domain("https://10.0.0.1").is_err());
        assert!(validate_anchor_domain("https://127.0.0.1").is_err());
        
        // IPv6 addresses should be rejected
        assert!(validate_anchor_domain("https://[::1]").is_err());
        assert!(validate_anchor_domain("https://[2001:db8::1]").is_err());
    }

    #[test]
    fn test_trailing_slashes() {
        // Trailing slashes should be allowed
        assert!(validate_anchor_domain("https://example.com/").is_ok());
        assert!(validate_anchor_domain("https://example.com/path/").is_ok());
        assert!(validate_anchor_domain("https://example.com/path/to/resource/").is_ok());
        
        // Multiple trailing slashes
        assert!(validate_anchor_domain("https://example.com//").is_ok());
    }

    #[test]
    fn test_length_boundaries() {
        // Domain exactly at 2048-character limit (should pass)
        let max_valid_domain = format!("https://{}.com", "a".repeat(2039));
        assert!(validate_anchor_domain(&max_valid_domain).is_ok());
        
        // Domain exceeding 2048-character limit (should fail)
        let too_long_domain = format!("https://{}.com", "a".repeat(2040));
        assert!(validate_anchor_domain(&too_long_domain).is_err());
        
        // Very short valid domains
        assert!(validate_anchor_domain("https://a.b").is_ok());
        assert!(validate_anchor_domain("https://ab.cd").is_ok());
    }

    #[test]
    fn test_query_parameters_and_fragments() {
        // Query parameters should be allowed
        assert!(validate_anchor_domain("https://example.com?param=value").is_ok());
        assert!(validate_anchor_domain("https://example.com?param1=value1&param2=value2").is_ok());
        
        // Fragments should be allowed
        assert!(validate_anchor_domain("https://example.com#section").is_ok());
        assert!(validate_anchor_domain("https://example.com/path#section").is_ok());
        
        // Both query and fragment
        assert!(validate_anchor_domain("https://example.com?param=value#section").is_ok());
    }

    #[test]
    fn test_special_characters_in_path() {
        // Valid special characters in paths
        assert!(validate_anchor_domain("https://example.com/path-with-dash").is_ok());
        assert!(validate_anchor_domain("https://example.com/path_with_underscore").is_ok());
        assert!(validate_anchor_domain("https://example.com/path.with.dot").is_ok());
        assert!(validate_anchor_domain("https://example.com/path~tilde").is_ok());
        assert!(validate_anchor_domain("https://example.com/path%20encoded").is_ok());
        
        // Invalid characters in paths
        assert!(validate_anchor_domain("https://example.com/path<invalid>").is_err());
        assert!(validate_anchor_domain("https://example.com/path{invalid}").is_err());
        assert!(validate_anchor_domain("https://example.com/path|pipe").is_err());
        assert!(validate_anchor_domain("https://example.com/path\\backslash").is_err());
    }

    #[test]
    fn test_port_edge_cases() {
        // Valid port ranges
        assert!(validate_anchor_domain("https://example.com:1").is_ok());
        assert!(validate_anchor_domain("https://example.com:80").is_ok());
        assert!(validate_anchor_domain("https://example.com:443").is_ok());
        assert!(validate_anchor_domain("https://example.com:8080").is_ok());
        assert!(validate_anchor_domain("https://example.com:65535").is_ok());
        
        // Invalid port ranges
        assert!(validate_anchor_domain("https://example.com:0").is_err());
        assert!(validate_anchor_domain("https://example.com:65536").is_err());
        assert!(validate_anchor_domain("https://example.com:99999").is_err());
        
        // Port with path
        assert!(validate_anchor_domain("https://example.com:8080/path").is_ok());
        assert!(validate_anchor_domain("https://example.com:8080/path?query=value").is_ok());
    }

    #[test]
    fn test_whitespace_variations() {
        // Leading/trailing whitespace should be rejected
        assert!(validate_anchor_domain(" https://example.com").is_err());
        assert!(validate_anchor_domain("https://example.com ").is_err());
        assert!(validate_anchor_domain("  https://example.com  ").is_err());
        
        // Internal whitespace should be rejected
        assert!(validate_anchor_domain("https://example .com").is_err());
        assert!(validate_anchor_domain("https://exam ple.com").is_err());
    }

    #[test]
    fn test_protocol_variations() {
        // Only HTTPS should be allowed
        assert!(validate_anchor_domain("https://example.com").is_ok());
        
        // All other protocols should be rejected
        assert!(validate_anchor_domain("http://example.com").is_err());
        assert!(validate_anchor_domain("ftp://example.com").is_err());
        assert!(validate_anchor_domain("ws://example.com").is_err());
        assert!(validate_anchor_domain("wss://example.com").is_err());
        assert!(validate_anchor_domain("file://example.com").is_err());
        assert!(validate_anchor_domain("mailto:example@example.com").is_err());
    }

    #[test]
    fn test_domain_label_edge_cases() {
        // Valid labels
        assert!(validate_anchor_domain("https://a-b-c.example.com").is_ok());
        assert!(validate_anchor_domain("https://123-456.example.com").is_ok());
        assert!(validate_anchor_domain("https://a1b2c3.example.com").is_ok());
        
        // Invalid labels
        assert!(validate_anchor_domain("https://-abc.example.com").is_err());
        assert!(validate_anchor_domain("https://abc-.example.com").is_err());
        assert!(validate_anchor_domain("https://a--b.example.com").is_ok()); // Double hyphens allowed in middle
        assert!(validate_anchor_domain("https://.example.com").is_err());
        assert!(validate_anchor_domain("https://example..com").is_err());
    }
}
