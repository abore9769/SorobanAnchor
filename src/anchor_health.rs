//! Off-chain helpers for anchor health monitoring and proof-of-possession.
//!
//! # Proof-of-possession protocol
//!
//! The proof-of-possession (PoP) flow lets an anchor prove it controls the
//! endpoint it advertises without requiring a live HTTP round-trip from the
//! contract itself (Soroban contracts cannot make outbound HTTP calls).
//!
//! ## Flow
//!
//! ```text
//! 1. Anchor publishes a challenge nonce in its stellar.toml:
//!       ANCHOR_PROOF_CHALLENGE = "<hex-encoded 32-byte nonce>"
//!
//! 2. Off-chain monitor fetches the challenge and computes:
//!       proof_hash = SHA-256(challenge_bytes || endpoint_bytes)
//!
//! 3. Anchor calls AnchorKitContract::register_endpoint_proof(
//!       anchor, endpoint, proof_hash)
//!    on-chain, binding its Stellar identity to the endpoint.
//!
//! 4. Any verifier calls AnchorKitContract::verify_endpoint_proof(
//!       anchor, proof_hash)
//!    to confirm the hash matches and mark the record as verified.
//! ```
//!
//! The helpers in this module implement step 2 and provide utilities for
//! health-event recording that wrap the contract calls.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

// ---------------------------------------------------------------------------
// Proof-of-possession helpers
// ---------------------------------------------------------------------------

/// Compute the proof-of-possession hash that an anchor must submit to
/// [`AnchorKitContract::register_endpoint_proof`].
///
/// `proof_hash = SHA-256(challenge_bytes || endpoint_bytes)`
///
/// # Arguments
///
/// * `challenge` - Raw bytes of the nonce published by the anchor
///   (e.g. hex-decoded `ANCHOR_PROOF_CHALLENGE` from `stellar.toml`).
/// * `endpoint`  - The endpoint URL string the anchor is proving ownership of.
///
/// # Returns
///
/// A 32-byte array containing the SHA-256 digest.
///
/// # Examples
///
/// ```rust
/// use anchorkit::anchor_health::compute_pop_hash;
///
/// let challenge = b"deadbeefdeadbeefdeadbeefdeadbeef";
/// let endpoint  = "https://anchor.example.com";
/// let hash = compute_pop_hash(challenge, endpoint);
/// assert_eq!(hash.len(), 32);
/// ```
pub fn compute_pop_hash(challenge: &[u8], endpoint: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(challenge);
    hasher.update(endpoint.as_bytes());
    hasher.finalize().into()
}

/// Verify that a stored proof hash matches the expected value recomputed from
/// `challenge` and `endpoint`.
///
/// Returns `true` when the hashes match — the anchor controls the endpoint.
///
/// # Arguments
///
/// * `stored_hash` - The 32-byte hash retrieved from the contract via
///   `AnchorKitContract::get_endpoint_proof`.
/// * `challenge`   - The raw challenge bytes fetched from the anchor's
///   `stellar.toml`.
/// * `endpoint`    - The endpoint URL to verify.
///
/// # Examples
///
/// ```rust
/// use anchorkit::anchor_health::{compute_pop_hash, verify_pop_challenge};
///
/// let challenge = b"deadbeefdeadbeefdeadbeefdeadbeef";
/// let endpoint  = "https://anchor.example.com";
/// let hash = compute_pop_hash(challenge, endpoint);
///
/// assert!(verify_pop_challenge(&hash, challenge, endpoint));
/// assert!(!verify_pop_challenge(&hash, b"wrongchallenge00wrongchallenge00", endpoint));
/// assert!(!verify_pop_challenge(&hash, challenge, "https://other.example.com"));
/// ```
pub fn verify_pop_challenge(
    stored_hash: &[u8; 32],
    challenge: &[u8],
    endpoint: &str,
) -> bool {
    let expected = compute_pop_hash(challenge, endpoint);
    // Constant-time comparison to prevent timing attacks.
    constant_time_eq(stored_hash, &expected)
}

/// Constant-time byte-slice equality check.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// Health event helpers
// ---------------------------------------------------------------------------

/// Outcome of a single anchor endpoint interaction, used as input to
/// [`classify_health_event`].
#[derive(Debug, Clone, PartialEq)]
pub enum EndpointOutcome {
    /// The call succeeded and returned a valid response.
    Success,
    /// The call failed (network error, timeout, invalid response, etc.).
    Failure(String),
}

impl EndpointOutcome {
    /// Returns `true` for [`EndpointOutcome::Success`].
    pub fn is_success(&self) -> bool {
        matches!(self, EndpointOutcome::Success)
    }

    /// Returns the failure reason, or an empty string for success.
    pub fn failure_reason(&self) -> &str {
        match self {
            EndpointOutcome::Success => "",
            EndpointOutcome::Failure(r) => r.as_str(),
        }
    }
}

/// Classify a raw HTTP status code into an [`EndpointOutcome`].
///
/// Status codes 200–299 are treated as success; everything else is a failure.
///
/// # Examples
///
/// ```rust
/// use anchorkit::anchor_health::{classify_http_status, EndpointOutcome};
///
/// assert_eq!(classify_http_status(200), EndpointOutcome::Success);
/// assert_eq!(classify_http_status(204), EndpointOutcome::Success);
/// assert!(matches!(classify_http_status(404), EndpointOutcome::Failure(_)));
/// assert!(matches!(classify_http_status(500), EndpointOutcome::Failure(_)));
/// ```
pub fn classify_http_status(status: u16) -> EndpointOutcome {
    if (200..300).contains(&status) {
        EndpointOutcome::Success
    } else {
        EndpointOutcome::Failure(alloc::format!("HTTP {status}"))
    }
}

/// Compute an uptime percentage (0.0–100.0) from raw success/failure counts.
///
/// Returns `0.0` when `total == 0`.
///
/// # Examples
///
/// ```rust
/// use anchorkit::anchor_health::uptime_percent;
///
/// assert_eq!(uptime_percent(9, 1), 90.0_f64);
/// assert_eq!(uptime_percent(0, 0), 0.0_f64);
/// assert_eq!(uptime_percent(1, 0), 100.0_f64);
/// ```
pub fn uptime_percent(success: u64, failure: u64) -> f64 {
    let total = success + failure;
    if total == 0 {
        return 0.0;
    }
    (success as f64 / total as f64) * 100.0
}

/// Convert basis-point uptime (0–10 000) returned by the contract to a
/// human-readable percentage string with two decimal places.
///
/// # Examples
///
/// ```rust
/// use anchorkit::anchor_health::bps_to_percent_str;
///
/// assert_eq!(bps_to_percent_str(10_000), "100.00%");
/// assert_eq!(bps_to_percent_str(9_950),  "99.50%");
/// assert_eq!(bps_to_percent_str(0),      "0.00%");
/// ```
pub fn bps_to_percent_str(bps: u32) -> String {
    let whole = bps / 100;
    let frac = bps % 100;
    alloc::format!("{whole}.{frac:02}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_pop_hash_is_deterministic() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let h1 = compute_pop_hash(challenge, endpoint);
        let h2 = compute_pop_hash(challenge, endpoint);
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_pop_hash_differs_on_different_inputs() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let h1 = compute_pop_hash(challenge, "https://anchor.example.com");
        let h2 = compute_pop_hash(challenge, "https://other.example.com");
        assert_ne!(h1, h2);

        let h3 = compute_pop_hash(b"different_challenge_bytes_______", "https://anchor.example.com");
        assert_ne!(h1, h3);
    }

    #[test]
    fn verify_pop_challenge_success() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let hash = compute_pop_hash(challenge, endpoint);
        assert!(verify_pop_challenge(&hash, challenge, endpoint));
    }

    #[test]
    fn verify_pop_challenge_wrong_challenge_fails() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let hash = compute_pop_hash(challenge, endpoint);
        assert!(!verify_pop_challenge(&hash, b"wrong_challenge_bytes_32_chars__", endpoint));
    }

    #[test]
    fn verify_pop_challenge_wrong_endpoint_fails() {
        let challenge = b"test_challenge_bytes_32_chars___";
        let endpoint = "https://anchor.example.com";
        let hash = compute_pop_hash(challenge, endpoint);
        assert!(!verify_pop_challenge(&hash, challenge, "https://evil.example.com"));
    }

    #[test]
    fn classify_http_status_success_range() {
        for code in [200u16, 201, 204, 299] {
            assert_eq!(classify_http_status(code), EndpointOutcome::Success, "code={code}");
        }
    }

    #[test]
    fn classify_http_status_failure_range() {
        for code in [400u16, 404, 500, 503] {
            assert!(matches!(classify_http_status(code), EndpointOutcome::Failure(_)), "code={code}");
        }
    }

    #[test]
    fn uptime_percent_calculations() {
        assert_eq!(uptime_percent(0, 0), 0.0);
        assert_eq!(uptime_percent(1, 0), 100.0);
        assert_eq!(uptime_percent(0, 1), 0.0);
        assert!((uptime_percent(9, 1) - 90.0).abs() < 1e-9);
        assert!((uptime_percent(1, 1) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn bps_to_percent_str_formatting() {
        assert_eq!(bps_to_percent_str(10_000), "100.00%");
        assert_eq!(bps_to_percent_str(9_950), "99.50%");
        assert_eq!(bps_to_percent_str(5_000), "50.00%");
        assert_eq!(bps_to_percent_str(0), "0.00%");
        assert_eq!(bps_to_percent_str(1), "0.01%");
    }

    #[test]
    fn endpoint_outcome_helpers() {
        assert!(EndpointOutcome::Success.is_success());
        assert!(!EndpointOutcome::Failure("err".into()).is_success());
        assert_eq!(EndpointOutcome::Success.failure_reason(), "");
        assert_eq!(EndpointOutcome::Failure("timeout".into()).failure_reason(), "timeout");
    }
}
