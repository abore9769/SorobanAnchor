//! Ratings API anti-spam and replay-proof signature protocol (issue #306).
//!
//! # Canonical message format
//! ```text
//! domain:supply-link:ratings
//! action:submit_rating
//! product_id:<product_id>
//! wallet:<wallet_address>
//! score:<1-5>
//! nonce:<hex-encoded 16-byte random nonce>
//! expires_at:<unix_timestamp>
//! ```
//! The fields are joined with `\n` and the resulting string is what the wallet
//! signs. The server verifies the signature, checks expiry, and records the
//! nonce to prevent replay.
//!
//! # Duplicate policy
//! One wallet may submit at most one rating per product. A second attempt
//! returns `RatingError::DuplicateSubmission`.

extern crate alloc;
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const DOMAIN_PREFIX: &str = "domain:supply-link:ratings";
pub const ACTION: &str = "action:submit_rating";
/// Maximum seconds a signed payload remains valid after `expires_at`.
pub const EXPIRY_WINDOW_SECONDS: u64 = 300; // 5 minutes

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum RatingError {
    /// Signature payload has expired.
    Expired,
    /// Nonce was already consumed (replay attack).
    ReplayDetected,
    /// Wallet already rated this product.
    DuplicateSubmission,
    /// Signature verification failed (tampered payload).
    InvalidSignature,
    /// Score outside 1–5 range.
    InvalidScore,
}

// ---------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------

/// Structured rating submission payload.
#[derive(Debug, Clone)]
pub struct RatingPayload {
    pub product_id: String,
    pub wallet: String,
    pub score: u8,
    pub nonce: String,
    pub expires_at: u64,
}

impl RatingPayload {
    /// Produce the canonical string that the wallet must sign.
    pub fn canonical_message(&self) -> String {
        alloc::format!(
            "{}\n{}\nproduct_id:{}\nwallet:{}\nscore:{}\nnonce:{}\nexpires_at:{}",
            DOMAIN_PREFIX,
            ACTION,
            self.product_id,
            self.wallet,
            self.score,
            self.nonce,
            self.expires_at,
        )
    }
}

// ---------------------------------------------------------------------------
// Signature verifier trait
// ---------------------------------------------------------------------------

pub trait SignatureVerifier {
    /// Returns `true` if `signature` is a valid signature of `message` by `wallet`.
    fn verify(&self, wallet: &str, message: &str, signature: &str) -> bool;
}

/// Always-valid verifier for unit tests.
pub struct AlwaysValidVerifier;
impl SignatureVerifier for AlwaysValidVerifier {
    fn verify(&self, _: &str, _: &str, _: &str) -> bool { true }
}

/// Always-invalid verifier for tamper tests.
pub struct AlwaysInvalidVerifier;
impl SignatureVerifier for AlwaysInvalidVerifier {
    fn verify(&self, _: &str, _: &str, _: &str) -> bool { false }
}

// ---------------------------------------------------------------------------
// Ratings service
// ---------------------------------------------------------------------------

pub struct RatingsService<V: SignatureVerifier> {
    verifier: V,
    /// Consumed nonces: key = `"<wallet>:<nonce>"`.
    used_nonces: BTreeMap<String, ()>,
    /// One rating per (wallet, product_id).
    submissions: BTreeMap<String, u8>,
}

impl<V: SignatureVerifier> RatingsService<V> {
    pub fn new(verifier: V) -> Self {
        Self {
            verifier,
            used_nonces: BTreeMap::new(),
            submissions: BTreeMap::new(),
        }
    }

    /// Submit a rating. `now` is the current Unix timestamp.
    pub fn submit(
        &mut self,
        payload: &RatingPayload,
        signature: &str,
        now: u64,
    ) -> Result<(), RatingError> {
        if payload.score < 1 || payload.score > 5 {
            return Err(RatingError::InvalidScore);
        }

        if now > payload.expires_at {
            return Err(RatingError::Expired);
        }

        let nonce_key = alloc::format!("{}:{}", payload.wallet, payload.nonce);
        if self.used_nonces.contains_key(&nonce_key) {
            return Err(RatingError::ReplayDetected);
        }

        let dup_key = alloc::format!("{}:{}", payload.wallet, payload.product_id);
        if self.submissions.contains_key(&dup_key) {
            return Err(RatingError::DuplicateSubmission);
        }

        let message = payload.canonical_message();
        if !self.verifier.verify(&payload.wallet, &message, signature) {
            return Err(RatingError::InvalidSignature);
        }

        self.used_nonces.insert(nonce_key, ());
        self.submissions.insert(dup_key, payload.score);
        Ok(())
    }

    /// Look up the stored score for a (wallet, product_id) pair.
    pub fn get_score(&self, wallet: &str, product_id: &str) -> Option<u8> {
        self.submissions.get(&alloc::format!("{}:{}", wallet, product_id)).copied()
    }
}
