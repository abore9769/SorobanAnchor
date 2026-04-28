//! Secure file upload hardening (issue #305).
//!
//! Provides content-signature verification beyond MIME header checks, per-actor
//! and per-product upload quotas, safe filename normalisation, and an audit log
//! of every rejection. Malware scanning is modelled as an async quarantine step
//! via the `MalwareScanner` trait so real integrations can plug in ClamAV, etc.

extern crate alloc;
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024; // 10 MiB
pub const MAX_UPLOADS_PER_ACTOR: u32 = 50;
pub const MAX_UPLOADS_PER_PRODUCT: u32 = 20;

/// Allowed (magic-byte prefix, mime) pairs.
const ALLOWED_SIGNATURES: &[(&[u8], &str)] = &[
    (b"\x89PNG\r\n\x1a\n", "image/png"),
    (b"\xff\xd8\xff", "image/jpeg"),
    (b"%PDF-", "application/pdf"),
    (b"GIF87a", "image/gif"),
    (b"GIF89a", "image/gif"),
];

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum UploadError {
    FileTooLarge,
    MimeMismatch,
    QuotaExceeded { scope: String },
    Quarantined,
    InvalidFilename,
}

impl UploadError {
    pub fn as_str(&self) -> &str {
        match self {
            Self::FileTooLarge => "file_too_large",
            Self::MimeMismatch => "mime_mismatch",
            Self::QuotaExceeded { .. } => "quota_exceeded",
            Self::Quarantined => "quarantined",
            Self::InvalidFilename => "invalid_filename",
        }
    }
}

// ---------------------------------------------------------------------------
// Malware scanner trait (async quarantine model)
// ---------------------------------------------------------------------------

pub trait MalwareScanner {
    /// Returns `true` if the content is clean, `false` if it should be quarantined.
    fn scan(&self, content: &[u8]) -> bool;
}

/// Pass-through scanner used in tests / non-production builds.
pub struct NoOpScanner;
impl MalwareScanner for NoOpScanner {
    fn scan(&self, _: &[u8]) -> bool { true }
}

// ---------------------------------------------------------------------------
// Audit log entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UploadRejection {
    pub actor: String,
    pub product_id: String,
    pub filename: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Upload guard
// ---------------------------------------------------------------------------

pub struct UploadGuard<S: MalwareScanner> {
    scanner: S,
    actor_counts: BTreeMap<String, u32>,
    product_counts: BTreeMap<String, u32>,
    pub audit_log: Vec<UploadRejection>,
}

impl<S: MalwareScanner> UploadGuard<S> {
    pub fn new(scanner: S) -> Self {
        Self {
            scanner,
            actor_counts: BTreeMap::new(),
            product_counts: BTreeMap::new(),
            audit_log: Vec::new(),
        }
    }

    /// Validate and accept an upload. Returns the sanitised filename on success.
    pub fn accept(
        &mut self,
        actor: &str,
        product_id: &str,
        filename: &str,
        claimed_mime: &str,
        content: &[u8],
    ) -> Result<String, UploadError> {
        let safe_name = self
            .sanitise_filename(filename)
            .ok_or_else(|| {
                self.reject(actor, product_id, filename, UploadError::InvalidFilename.as_str());
                UploadError::InvalidFilename
            })?;

        if content.len() > MAX_FILE_BYTES {
            self.reject(actor, product_id, filename, UploadError::FileTooLarge.as_str());
            return Err(UploadError::FileTooLarge);
        }

        if !self.verify_content_signature(content, claimed_mime) {
            self.reject(actor, product_id, filename, UploadError::MimeMismatch.as_str());
            return Err(UploadError::MimeMismatch);
        }

        let actor_count = self.actor_counts.get(actor).copied().unwrap_or(0);
        if actor_count >= MAX_UPLOADS_PER_ACTOR {
            self.reject(actor, product_id, filename, UploadError::QuotaExceeded { scope: "actor".into() }.as_str());
            return Err(UploadError::QuotaExceeded { scope: "actor".into() });
        }

        let prod_count = self.product_counts.get(product_id).copied().unwrap_or(0);
        if prod_count >= MAX_UPLOADS_PER_PRODUCT {
            self.reject(actor, product_id, filename, UploadError::QuotaExceeded { scope: "product".into() }.as_str());
            return Err(UploadError::QuotaExceeded { scope: "product".into() });
        }

        if !self.scanner.scan(content) {
            self.reject(actor, product_id, filename, UploadError::Quarantined.as_str());
            return Err(UploadError::Quarantined);
        }

        *self.actor_counts.entry(actor.into()).or_insert(0) += 1;
        *self.product_counts.entry(product_id.into()).or_insert(0) += 1;
        Ok(safe_name)
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Verify magic bytes match the claimed MIME type.
    fn verify_content_signature(&self, content: &[u8], claimed_mime: &str) -> bool {
        ALLOWED_SIGNATURES
            .iter()
            .any(|(magic, mime)| *mime == claimed_mime && content.starts_with(magic))
    }

    /// Strip path components, reject traversal, replace unsafe chars.
    fn sanitise_filename(&self, name: &str) -> Option<String> {
        // Reject traversal attempts
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            return None;
        }
        let base: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        if base.is_empty() || base == "." {
            return None;
        }
        Some(base)
    }

    fn reject(&mut self, actor: &str, product_id: &str, filename: &str, reason: &str) {
        self.audit_log.push(UploadRejection {
            actor: actor.into(),
            product_id: product_id.into(),
            filename: filename.into(),
            reason: reason.into(),
        });
    }
}
