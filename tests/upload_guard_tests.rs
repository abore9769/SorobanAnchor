//! Tests for secure file upload hardening. Closes #305.

#[cfg(test)]
mod upload_guard_tests {
    use anchorkit::upload_guard::{
        MalwareScanner, NoOpScanner, UploadError, UploadGuard,
        MAX_UPLOADS_PER_ACTOR, MAX_UPLOADS_PER_PRODUCT,
    };

    fn png_bytes() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 16]);
        v
    }

    fn pdf_bytes() -> Vec<u8> {
        let mut v = b"%PDF-".to_vec();
        v.extend_from_slice(&[0u8; 16]);
        v
    }

    // ── Happy path ───────────────────────────────────────────────────────────

    #[test]
    fn valid_png_accepted_and_filename_sanitised() {
        let mut g = UploadGuard::new(NoOpScanner);
        let name = g.accept("alice", "prod-1", "photo.png", "image/png", &png_bytes()).unwrap();
        assert_eq!(name, "photo.png");
        assert!(g.audit_log.is_empty());
    }

    // ── Content-signature checks ─────────────────────────────────────────────

    #[test]
    fn wrong_mime_rejected() {
        let mut g = UploadGuard::new(NoOpScanner);
        // Content is PNG but claimed as PDF
        let err = g.accept("alice", "prod-1", "file.pdf", "application/pdf", &png_bytes()).unwrap_err();
        assert_eq!(err, UploadError::MimeMismatch);
        assert_eq!(g.audit_log[0].reason, "mime_mismatch");
    }

    #[test]
    fn plain_text_with_image_mime_rejected() {
        let mut g = UploadGuard::new(NoOpScanner);
        let err = g.accept("alice", "prod-1", "evil.png", "image/png", b"not a real png").unwrap_err();
        assert_eq!(err, UploadError::MimeMismatch);
    }

    // ── Filename sanitisation ────────────────────────────────────────────────

    #[test]
    fn path_traversal_rejected() {
        let mut g = UploadGuard::new(NoOpScanner);
        let err = g.accept("alice", "prod-1", "../etc/passwd", "image/png", &png_bytes()).unwrap_err();
        assert_eq!(err, UploadError::InvalidFilename);
    }

    #[test]
    fn absolute_path_rejected() {
        let mut g = UploadGuard::new(NoOpScanner);
        let err = g.accept("alice", "prod-1", "/etc/passwd", "image/png", &png_bytes()).unwrap_err();
        assert_eq!(err, UploadError::InvalidFilename);
    }

    #[test]
    fn unsafe_chars_in_filename_sanitised() {
        let mut g = UploadGuard::new(NoOpScanner);
        let name = g.accept("alice", "prod-1", "my file (1).png", "image/png", &png_bytes()).unwrap();
        assert!(!name.contains(' '));
        assert!(!name.contains('('));
    }

    // ── Quota controls ───────────────────────────────────────────────────────

    #[test]
    fn actor_quota_enforced() {
        let mut g = UploadGuard::new(NoOpScanner);
        for i in 0..MAX_UPLOADS_PER_ACTOR {
            g.accept("bob", &format!("p{i}"), "f.png", "image/png", &png_bytes()).unwrap();
        }
        let err = g.accept("bob", "overflow", "f.png", "image/png", &png_bytes()).unwrap_err();
        assert_eq!(err, UploadError::QuotaExceeded { scope: "actor".into() });
    }

    #[test]
    fn product_quota_enforced() {
        let mut g = UploadGuard::new(NoOpScanner);
        for i in 0..MAX_UPLOADS_PER_PRODUCT {
            g.accept(&format!("user{i}"), "shared-prod", "f.pdf", "application/pdf", &pdf_bytes()).unwrap();
        }
        let err = g.accept("new-user", "shared-prod", "f.pdf", "application/pdf", &pdf_bytes()).unwrap_err();
        assert_eq!(err, UploadError::QuotaExceeded { scope: "product".into() });
    }

    // ── Malware quarantine ───────────────────────────────────────────────────

    struct AlwaysInfected;
    impl MalwareScanner for AlwaysInfected {
        fn scan(&self, _: &[u8]) -> bool { false }
    }

    #[test]
    fn malicious_content_quarantined() {
        let mut g = UploadGuard::new(AlwaysInfected);
        let err = g.accept("alice", "prod-1", "virus.png", "image/png", &png_bytes()).unwrap_err();
        assert_eq!(err, UploadError::Quarantined);
        assert_eq!(g.audit_log[0].reason, "quarantined");
    }

    // ── Audit log ────────────────────────────────────────────────────────────

    #[test]
    fn every_rejection_is_logged() {
        let mut g = UploadGuard::new(NoOpScanner);
        let _ = g.accept("alice", "p1", "../bad", "image/png", &png_bytes());
        let _ = g.accept("alice", "p1", "ok.png", "application/pdf", &png_bytes());
        assert_eq!(g.audit_log.len(), 2);
    }
}
