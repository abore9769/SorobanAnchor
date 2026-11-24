//! Example demonstrating anchor domain validation
//!
//! This example shows how to use the domain validator utility
//! to validate anchor domain URLs before making requests.
//!
//! Mode: Offline. Validation is purely syntactic; no DNS lookups or HTTP
//!   requests are made.
//!
//! Prerequisites:
//!   Binary/build: Rust toolchain (cargo)
//!   Environment:  None
//!   Network:      None, except that the first cargo build may download crates
//!
//! Run:
//!   cargo run --example domain_validation_example

use anchorkit::{validate_anchor_domain, Error};

fn main() {
    println!("=== Anchor Domain Validation Examples ===\n");

    // Valid domains
    let valid_domains = vec![
        "https://api.stellar.org",
        "https://anchor.example.com",
        "https://api.anchor.com:8080",
        "https://anchor.com/api/v1",
        "https://anchor.com/sep24?asset=USDC",
    ];

    println!("Valid domains:");
    for domain in valid_domains {
        match validate_anchor_domain(domain) {
            Ok(()) => println!("  ✓ {}", domain),
            Err(e) => println!("  ✗ {} - Error: {:?}", domain, e),
        }
    }

    // Invalid domains
    let invalid_domains = vec![
        "http://insecure.com",           // Not HTTPS
        "example.com",                    // Missing protocol
        "https://",                       // No domain
        "https://example",                // No TLD
        "https://.example.com",           // Leading dot
        "https://example..com",           // Double dots
        "https://example.com:99999",      // Invalid port
    ];

    println!("\nInvalid domains:");
    for domain in invalid_domains {
        match validate_anchor_domain(domain) {
            Ok(()) => println!("  ✗ {} - Should have failed!", domain),
            Err(ref e) if e.code == anchorkit::ErrorCode::InvalidEndpointFormat => println!("  ✓ {} - Correctly rejected", domain),
            Err(e) => println!("  ? {} - Unexpected error: {:?}", domain, e),
        }
    }

    println!("\n=== Validation Complete ===");
}
