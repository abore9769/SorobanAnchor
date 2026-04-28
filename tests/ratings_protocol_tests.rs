//! Tests for the ratings anti-spam and replay-proof signature protocol.
//! Closes #306.

#[cfg(test)]
mod ratings_protocol_tests {
    use anchorkit::ratings_protocol::{
        AlwaysInvalidVerifier, AlwaysValidVerifier, RatingError, RatingPayload, RatingsService,
        DOMAIN_PREFIX, ACTION,
    };

    fn payload(nonce: &str, expires_at: u64) -> RatingPayload {
        RatingPayload {
            product_id: "prod-abc".into(),
            wallet: "GWALLETXXX".into(),
            score: 4,
            nonce: nonce.into(),
            expires_at,
        }
    }

    // ── Canonical message format ─────────────────────────────────────────────

    #[test]
    fn canonical_message_contains_all_fields() {
        let p = payload("nonce123", 9999);
        let msg = p.canonical_message();
        assert!(msg.starts_with(DOMAIN_PREFIX));
        assert!(msg.contains(ACTION));
        assert!(msg.contains("product_id:prod-abc"));
        assert!(msg.contains("wallet:GWALLETXXX"));
        assert!(msg.contains("score:4"));
        assert!(msg.contains("nonce:nonce123"));
        assert!(msg.contains("expires_at:9999"));
    }

    // ── Happy path ───────────────────────────────────────────────────────────

    #[test]
    fn valid_submission_accepted() {
        let mut svc = RatingsService::new(AlwaysValidVerifier);
        let p = payload("n1", 1000);
        svc.submit(&p, "sig", 500).unwrap();
        assert_eq!(svc.get_score("GWALLETXXX", "prod-abc"), Some(4));
    }

    // ── Expiry ───────────────────────────────────────────────────────────────

    #[test]
    fn expired_payload_rejected() {
        let mut svc = RatingsService::new(AlwaysValidVerifier);
        let p = payload("n2", 500); // expires_at = 500
        let err = svc.submit(&p, "sig", 501).unwrap_err(); // now = 501
        assert_eq!(err, RatingError::Expired);
    }

    #[test]
    fn payload_at_exact_expiry_accepted() {
        let mut svc = RatingsService::new(AlwaysValidVerifier);
        let p = payload("n3", 1000);
        svc.submit(&p, "sig", 1000).unwrap(); // now == expires_at is still valid
    }

    // ── Replay detection ─────────────────────────────────────────────────────

    #[test]
    fn replay_of_same_nonce_rejected() {
        let mut svc = RatingsService::new(AlwaysValidVerifier);
        let p = payload("nonce-replay", 9999);
        svc.submit(&p, "sig", 100).unwrap();

        // Different product so it's not a duplicate-submission error, but same nonce
        let p2 = RatingPayload {
            product_id: "prod-other".into(),
            wallet: "GWALLETXXX".into(),
            score: 3,
            nonce: "nonce-replay".into(),
            expires_at: 9999,
        };
        let err = svc.submit(&p2, "sig", 100).unwrap_err();
        assert_eq!(err, RatingError::ReplayDetected);
    }

    // ── Duplicate wallet-product submission ──────────────────────────────────

    #[test]
    fn duplicate_wallet_product_rejected() {
        let mut svc = RatingsService::new(AlwaysValidVerifier);
        svc.submit(&payload("n4", 9999), "sig", 100).unwrap();
        let p2 = RatingPayload { nonce: "n5".into(), ..payload("n5", 9999) };
        let err = svc.submit(&p2, "sig", 100).unwrap_err();
        assert_eq!(err, RatingError::DuplicateSubmission);
    }

    // ── Tampered payload ─────────────────────────────────────────────────────

    #[test]
    fn invalid_signature_rejected() {
        let mut svc = RatingsService::new(AlwaysInvalidVerifier);
        let err = svc.submit(&payload("n6", 9999), "bad-sig", 100).unwrap_err();
        assert_eq!(err, RatingError::InvalidSignature);
    }

    // ── Score validation ─────────────────────────────────────────────────────

    #[test]
    fn score_zero_rejected() {
        let mut svc = RatingsService::new(AlwaysValidVerifier);
        let p = RatingPayload { score: 0, ..payload("n7", 9999) };
        assert_eq!(svc.submit(&p, "sig", 100).unwrap_err(), RatingError::InvalidScore);
    }

    #[test]
    fn score_six_rejected() {
        let mut svc = RatingsService::new(AlwaysValidVerifier);
        let p = RatingPayload { score: 6, ..payload("n8", 9999) };
        assert_eq!(svc.submit(&p, "sig", 100).unwrap_err(), RatingError::InvalidScore);
    }
}
