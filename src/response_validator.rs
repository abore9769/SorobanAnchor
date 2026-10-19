//! Response schema validation for AnchorKit API responses.
//!
//! Validates that anchor API responses contain all required fields before
//! returning them to the SDK consumer. Throws [`Error::ValidationError`] on mismatch.
//!
//! # Schema versioning
//!//! Request record management: retention policies (#679) and export/archival (#680).
//!
//! This module provides:
//! - [`RequestRetentionPolicy`]: configurable rules for how long request records
//!   are kept and the maximum number to retain.
//! - [`RequestRecord`]: a structured record of a request/transaction event
//!   suitable for off-system archival.
//! - [`RequestRecordStore`]: an in-memory store (host-side) that enforces the
//!   retention policy and supports paginated batch export.
//!
//! ## Retention policy (#679)
//!
//! Operators configure a [`RequestRetentionPolicy`] with:
//! - `max_records`: hard cap on the number of records retained (0 = unlimited).
//! - `max_age_seconds`: maximum age in seconds; older records are pruned (0 = no age limit).
//! - `prune_on_write`: when `true`, the policy is enforced on every [`RequestRecordStore::push`].
//!
//! ## Export and archival (#680)
//!
//! [`RequestRecordStore::export_batch`] returns a window of records by cursor
//! position. Callers iterate through all records by advancing the cursor until
//! `ExportBatch::has_more` is `false`, then write the batches to off-system
//! storage (S3, a database, etc.).
//!
//! [`RequestRecordStore::archive_before`] moves records older than a given
//! timestamp out of the active store into a separate archive `Vec` so they are
//! no longer included in normal queries but remain available for inspection or
//! final export before deletion.

extern crate alloc;
//! Request deduplication for repeated operations (#681).
//!
//! Repeated submissions of the same logical request (e.g. a deposit initiation
//! retried by a client after a network hiccup) can produce duplicate work and
//! redundant side-effects. This module provides a lightweight deduplication
//! layer that collapses identical requests into a single execution path by
//! tracking a deduplicated key → cached result mapping.
//!
//! # Design
//!
//! * **Key-based deduplication.** A [`DeduplicationKey`] uniquely identifies a
//!   logical operation. Keys are intentionally caller-constructed so the
//!   deduplication policy is decoupled from transport concerns.
//! * **Result caching.** The first execution of a key stores either the
//!   success value or the error kind. Subsequent calls with the same key
//!   receive the cached outcome without re-running the operation.
//! * **TTL / expiry.** Each entry carries an expiry timestamp so stale
//!   results are not served indefinitely. [`DeduplicationStore::purge_expired`]
//!   cleans up entries older than their TTL.
//! * **No `std` dependency.** Uses `alloc::collections::BTreeMap` so the
//!   module can be compiled for `no_std` targets if needed.
//!
//! # Example
//!
//! ```rust
//! use anchorkit::request_deduplication::{DeduplicationStore, DeduplicationKey, DeduplicationResult};
//!
//! let mut store = DeduplicationStore::new(300); // 5-minute TTL
//! let key = DeduplicationKey::new("deposit", "txn-001");
//!
//! // First call — not deduplicated, run the operation.
//! assert!(!store.is_duplicate(&key, 1_000));
//! store.record_success(&key, "pending_external", 1_000);
//!
//! // Second call — deduplicated, returns cached outcome.
//! assert!(store.is_duplicate(&key, 1_001));
//! assert_eq!(
//!     store.cached_result(&key, 1_001),
//!     Some(DeduplicationResult::Success("pending_external".into())),
//! );
//! 
use alloc::{string::String, vec::Vec};

// ---------------------------------------------------------------------------
// RequestRetentionPolicy (#679)
// ---------------------------------------------------------------------------

/// Configurable retention policy for request records.
///
/// # Fields
///
/// - `max_records`: maximum number of records to keep.  `0` means no limit.
/// - `max_age_seconds`: discard records older than this many seconds.  `0` means no limit.
/// - `prune_on_write`: when `true`, [`RequestRecordStore::push`] enforces the
///   policy immediately after inserting the new record.
///
/// # Examples
///
/// ```rust
/// use anchorkit::request_record::RequestRetentionPolicy;
///
/// // Keep at most 1 000 records; no age-based pruning; enforce on every write.
/// let policy = RequestRetentionPolicy::new(1_000, 0, true);
/// assert_eq!(policy.max_records, 1_000);
/// assert!(!policy.has_age_limit());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct RequestRetentionPolicy {
    /// Maximum number of records to keep (0 = unlimited).
    pub max_records: usize,
    /// Maximum age in seconds for a record (0 = no age limit).
    pub max_age_seconds: u64,
    /// Enforce policy automatically on each `push` call.
    pub prune_on_write: bool,
}

impl RequestRetentionPolicy {
    /// Create a new retention policy.
    pub fn new(max_records: usize, max_age_seconds: u64, prune_on_write: bool) -> Self {
        Self { max_records, max_age_seconds, prune_on_write }
    }

    /// Returns `true` when an age-based limit is configured.
    pub fn has_age_limit(&self) -> bool {
        self.max_age_seconds > 0
    }

    /// Returns `true` when a record-count limit is configured.
    pub fn has_record_limit(&self) -> bool {
        self.max_records > 0
    }
}

impl Default for RequestRetentionPolicy {
    fn default() -> Self {
        // Sensible default: keep 10 000 records, no age pruning, enforce on write.
        Self::new(10_000, 0, true)
    }
}

// ---------------------------------------------------------------------------
// RequestRecord
// ---------------------------------------------------------------------------

/// A single request record eligible for retention management and archival.
///
/// Records are produced by higher-level callers (e.g. the host boundary replay
/// prevention layer or the transaction state tracker) and pushed into a
/// [`RequestRecordStore`].
#[derive(Clone, Debug, PartialEq)]
pub struct RequestRecord {
    /// Unique identifier for this record (e.g. the transaction ID).
    pub id: u64,
    /// Unix timestamp (seconds) when the request was received.
    pub timestamp: u64,
    /// Human-readable label for the operation (e.g. `"attest"`, `"quote"`).
    pub operation: String,
    /// The Stellar address that submitted the request.
    pub actor: String,
    /// Outcome tag: `"accepted"`, `"rejected"`, `"replayed"`, etc.
    pub outcome: String,
    /// Optional free-form detail (error message, routing reason, …).
    pub detail: Option<String>,
}

impl RequestRecord {
    /// Construct a new record.
    pub fn new(
        id: u64,
        timestamp: u64,
        operation: impl Into<String>,
        actor: impl Into<String>,
        outcome: impl Into<String>,
        detail: Option<impl Into<String>>,
    ) -> Self {
        Self {
            id,
            timestamp,
            operation: operation.into(),
            actor: actor.into(),
            outcome: outcome.into(),
            detail: detail.map(Into::into),
        }
    }
}

// ---------------------------------------------------------------------------
// ExportBatch (#680)
// ---------------------------------------------------------------------------

/// A paginated batch of [`RequestRecord`]s returned by
/// [`RequestRecordStore::export_batch`].
#[derive(Clone, Debug)]
pub struct ExportBatch {
    /// The records in this batch.
    pub records: Vec<RequestRecord>,
    /// Cursor value to pass as `start_cursor` in the next call.
    /// Meaningless when `has_more` is `false`.
    pub next_cursor: usize,
    /// `true` when there are more records beyond this batch.
    pub has_more: bool,
}

// ---------------------------------------------------------------------------
// RequestRecordStore
// ---------------------------------------------------------------------------

/// Host-side store for request records with retention and export support.
///
/// Combines:
/// - An active record buffer (`records`) subject to the [`RequestRetentionPolicy`].
/// - An archive buffer (`archive`) holding records moved out by
///   [`archive_before`](Self::archive_before).
///
/// # Examples
///
/// ```rust
/// use anchorkit::request_record::{RequestRecord, RequestRecordStore, RequestRetentionPolicy};
///
/// let policy = RequestRetentionPolicy::new(5, 0, true);
/// let mut store = RequestRecordStore::new(policy);
///
/// for i in 0..10_u64 {
///     store.push(RequestRecord::new(i, 1_000 + i, "attest", "GXXX", "accepted", None::<&str>), 0);
/// }
/// // Only 5 most-recent records are kept.
/// assert_eq!(store.len(), 5);
/// ```
#[derive(Debug, Default)]
pub struct RequestRecordStore {
    /// Active records, ordered by insertion (oldest first).
    records: Vec<RequestRecord>,
    /// Archived records (moved out by `archive_before`).
    archive: Vec<RequestRecord>,
    /// The active retention policy.
    pub policy: RequestRetentionPolicy,
}

impl RequestRecordStore {
    /// Create a new store with the given retention policy.
    pub fn new(policy: RequestRetentionPolicy) -> Self {
        Self { records: Vec::new(), archive: Vec::new(), policy }
    }

    /// Push a new record into the store.
    ///
    /// If `policy.prune_on_write` is `true`, the retention policy is enforced
    /// immediately after insertion. `now_secs` is the current Unix timestamp
    /// used for age-based pruning; pass `0` to skip age pruning on this write.
    pub fn push(&mut self, record: RequestRecord, now_secs: u64) {
        self.records.push(record);
        if self.policy.prune_on_write {
            self.enforce_policy(now_secs);
        }
    }

    /// Enforce the retention policy against the active record buffer.
    ///
    /// `now_secs` is the current Unix timestamp used for age-based pruning.
    /// Pass `0` to skip age-based pruning regardless of the policy setting.
    ///
    /// Returns the number of records removed.
    pub fn enforce_policy(&mut self, now_secs: u64) -> usize {
        let before = self.records.len();

        // Age-based pruning (oldest first).
        if self.policy.has_age_limit() && now_secs > 0 {
            let cutoff = now_secs.saturating_sub(self.policy.max_age_seconds);
            self.records.retain(|r| r.timestamp >= cutoff);
        }

        // Record-count cap: drop oldest records when over limit.
        if self.policy.has_record_limit() {
            let limit = self.policy.max_records;
            while self.records.len() > limit {
                self.records.remove(0);
            }
        }

        before.saturating_sub(self.records.len())
    }

    /// Update the retention policy and immediately apply it.
    ///
    /// `now_secs` is used for age-based pruning (pass `0` to skip).
    pub fn set_policy(&mut self, policy: RequestRetentionPolicy, now_secs: u64) {
        self.policy = policy;
        self.enforce_policy(now_secs);
    }

    /// Return the number of active (non-archived) records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Return `true` when the active record buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Return the number of archived records.
    pub fn archive_len(&self) -> usize {
        self.archive.len()
    }

    // ── Export / archival (#680) ─────────────────────────────────────────────

    /// Maximum number of records returned by one export page.
    pub const MAX_EXPORT_PAGE_SIZE: usize = 100;

    /// Export a paginated batch of active records starting at `start_cursor`.
    ///
    /// `batch_size` is capped at 100 to prevent unbounded allocations.
    ///
    /// # Arguments
    ///
    /// * `start_cursor` – Zero-based index into the active record buffer.
    /// * `batch_size`   – Maximum number of records per batch (capped at 100).
    ///
    /// # Returns
    ///
    /// An [`ExportBatch`] with the records and a cursor for the next call.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use anchorkit::request_record::{RequestRecord, RequestRecordStore, RequestRetentionPolicy};
    ///
    /// let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
    /// for i in 0..25_u64 {
    ///     store.push(RequestRecord::new(i, 1_000 + i, "attest", "GXXX", "accepted", None::<&str>));
    /// }
    ///
    /// let batch = store.export_batch(0, 10);
    /// assert_eq!(batch.records.len(), 10);
    /// assert!(batch.has_more);
    ///
    /// let last = store.export_batch(batch.next_cursor, 10);
    /// assert_eq!(last.records.len(), 10);
    /// assert!(last.has_more);
    ///
    /// let tail = store.export_batch(last.next_cursor, 10);
    /// assert_eq!(tail.records.len(), 5);
    /// assert!(!tail.has_more);
    /// ```
    pub fn export_batch(&self, start_cursor: usize, batch_size: usize) -> ExportBatch {
        let effective_size = batch_size.min(Self::MAX_EXPORT_PAGE_SIZE);

        if start_cursor >= self.records.len() {
            return ExportBatch {
                records: Vec::new(),
                next_cursor: start_cursor,
                has_more: false,
            };
        }

        let end = start_cursor
            .saturating_add(effective_size)
            .min(self.records.len());
        let records = self.records[start_cursor..end].to_vec();
        let next_cursor = end;
        let has_more = next_cursor < self.records.len();

        ExportBatch { records, next_cursor, has_more }
    }

    /// Export all active records in a single allocation (no pagination).
    ///
    /// Suitable for small stores where a single snapshot is acceptable.
    pub fn export_all(&self) -> Vec<RequestRecord> {
        self.records.clone()
    }

    /// Move all active records whose `timestamp < cutoff_secs` into the
    /// archive buffer.
    ///
    /// Archived records are no longer included in [`len`](Self::len),
    /// [`export_batch`](Self::export_batch), or [`export_all`](Self::export_all).
    /// They remain accessible via [`archive_export`](Self::archive_export).
    ///
    /// Returns the number of records archived.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use anchorkit::request_record::{RequestRecord, RequestRecordStore, RequestRetentionPolicy};
    ///
    /// let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
    /// store.push(RequestRecord::new(1, 1_000, "attest", "GXXX", "accepted", None::<&str>));
    /// store.push(RequestRecord::new(2, 2_000, "attest", "GXXX", "accepted", None::<&str>));
    /// store.push(RequestRecord::new(3, 3_000, "attest", "GXXX", "accepted", None::<&str>));
    ///
    /// let archived = store.archive_before(2_500);
    /// assert_eq!(archived, 2);
    /// assert_eq!(store.len(), 1);
    /// assert_eq!(store.archive_len(), 2);
    /// ```
    pub fn archive_before(&mut self, cutoff_secs: u64) -> usize {
        let mut to_archive: Vec<RequestRecord> = Vec::new();
        let mut remaining: Vec<RequestRecord> = Vec::new();

        for record in self.records.drain(..) {
            if record.timestamp < cutoff_secs {
                to_archive.push(record);
            } else {
                remaining.push(record);
            }
        }

        let count = to_archive.len();
        self.archive.extend(to_archive);
        self.records = remaining;
        count
    }

    /// Export a paginated batch from the archive buffer.
    ///
    /// Same pagination contract as [`export_batch`](Self::export_batch).
    pub fn archive_export(&self, start_cursor: usize, batch_size: usize) -> ExportBatch {
        let effective_size = batch_size.min(Self::MAX_EXPORT_PAGE_SIZE);

        if start_cursor >= self.archive.len() {
            return ExportBatch {
                records: Vec::new(),
                next_cursor: start_cursor,
                has_more: false,
            };
        }

        let end = start_cursor
            .saturating_add(effective_size)
            .min(self.archive.len());
        let records = self.archive[start_cursor..end].to_vec();
        let next_cursor = end;
        let has_more = next_cursor < self.archive.len();

        ExportBatch { records, next_cursor, has_more }
    }

    /// Drain and discard all archived records.
    ///
    /// Call this after a successful off-system write to reclaim memory.
    /// Returns the number of records discarded.
    pub fn clear_archive(&mut self) -> usize {
        let count = self.archive.len();
        self.archive.clear();
        count
    }

    /// Look up a single active record by its `id` field.
    pub fn get_by_id(&self, id: u64) -> Option<&RequestRecord> {
        self.records.iter().find(|r| r.id == id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(id: u64, ts: u64) -> RequestRecord {
        RequestRecord::new(id, ts, "attest", "GXXX", "accepted", None::<&str>)
    }

    // ── RetentionPolicy ─────────────────────────────────────────────────────

    #[test]
    fn test_policy_default_has_record_limit() {
        let p = RequestRetentionPolicy::default();
        assert!(p.has_record_limit());
        assert!(!p.has_age_limit());
        assert!(p.prune_on_write);
    }

    #[test]
    fn test_policy_unlimited_no_limits() {
        let p = RequestRetentionPolicy::new(0, 0, false);
        assert!(!p.has_record_limit());
        assert!(!p.has_age_limit());
    }

    // ── push + enforce_policy (record cap) ─────────────────────────────────

    #[test]
    fn test_push_enforces_record_cap() {
        let policy = RequestRetentionPolicy::new(3, 0, true);
        let mut store = RequestRecordStore::new(policy);
        for i in 0..5_u64 {
            store.push(make_record(i, 1_000 + i), 0);
        }
        // Only the 3 most-recent records should remain.
        assert_eq!(store.len(), 3);
        assert_eq!(store.records[0].id, 2);
        assert_eq!(store.records[2].id, 4);
    }

    #[test]
    fn test_push_no_cap_unlimited() {
        let policy = RequestRetentionPolicy::new(0, 0, true);
        let mut store = RequestRecordStore::new(policy);
        for i in 0..50_u64 {
            store.push(make_record(i, i), 0);
        }
        assert_eq!(store.len(), 50);
    }

    #[test]
    fn test_push_enforces_age_pruning_on_write() {
        // max_age_seconds=100: records older than (now - 100) should be pruned on push.
        let policy = RequestRetentionPolicy::new(0, 100, true);
        let mut store = RequestRecordStore::new(policy);

        // Push a record that is already older than the age limit at write time.
        store.push(make_record(1, 800), 1_000); // cutoff = 900; ts=800 < 900 → pruned
        assert_eq!(store.len(), 0, "record older than age limit must be pruned on write");

        // Push a record that is within the age limit.
        store.push(make_record(2, 950), 1_000); // ts=950 >= 900 → kept
        assert_eq!(store.len(), 1, "record within age limit must be retained");
    }

    // ── enforce_policy (age-based) ───────────────────────────────────────────

    #[test]
    fn test_enforce_policy_age_prunes_old_records() {
        let policy = RequestRetentionPolicy::new(0, 100, false);
        let mut store = RequestRecordStore::new(policy);
        store.records.push(make_record(1, 900));  // old
        store.records.push(make_record(2, 950));  // old
        store.records.push(make_record(3, 1_050)); // recent

        let removed = store.enforce_policy(1_050);
        assert_eq!(removed, 2);
        assert_eq!(store.len(), 1);
        assert_eq!(store.records[0].id, 3);
    }

    #[test]
    fn test_enforce_policy_age_zero_skips_pruning() {
        let policy = RequestRetentionPolicy::new(0, 100, false);
        let mut store = RequestRecordStore::new(policy);
        store.records.push(make_record(1, 100));
        // now_secs = 0 → skip age pruning
        let removed = store.enforce_policy(0);
        assert_eq!(removed, 0);
        assert_eq!(store.len(), 1);
    }

    // ── set_policy ───────────────────────────────────────────────────────────

    #[test]
    fn test_set_policy_applies_immediately() {
        let policy = RequestRetentionPolicy::new(0, 0, false);
        let mut store = RequestRecordStore::new(policy);
        for i in 0..20_u64 {
            store.records.push(make_record(i, i));
        }
        assert_eq!(store.len(), 20);

        store.set_policy(RequestRetentionPolicy::new(5, 0, true), 0);
        assert_eq!(store.len(), 5);
    }

    // ── export_batch ─────────────────────────────────────────────────────────

    #[test]
    fn test_export_batch_first_page() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        for i in 0..25_u64 {
            store.records.push(make_record(i, i));
        }

        let batch = store.export_batch(0, 10);
        assert_eq!(batch.records.len(), 10);
        assert_eq!(batch.next_cursor, 10);
        assert!(batch.has_more);
    }

    #[test]
    fn test_export_batch_last_page() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        for i in 0..25_u64 {
            store.records.push(make_record(i, i));
        }

        let batch = store.export_batch(20, 10);
        assert_eq!(batch.records.len(), 5);
        assert!(!batch.has_more);
    }

    #[test]
    fn test_export_batch_beyond_end_returns_empty() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        store.records.push(make_record(1, 1));

        let batch = store.export_batch(100, 10);
        assert!(batch.records.is_empty());
        assert!(!batch.has_more);
    }

    #[test]
    fn test_export_batch_capped_at_100() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        for i in 0..200_u64 {
            store.records.push(make_record(i, i));
        }

        let batch = store.export_batch(0, 200);
        assert_eq!(batch.records.len(), 100);
    }

    #[test]
    fn test_export_all_returns_full_copy() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        for i in 0..10_u64 {
            store.records.push(make_record(i, i));
        }
        let all = store.export_all();
        assert_eq!(all.len(), 10);
    }

    // ── archive_before ───────────────────────────────────────────────────────

    #[test]
    fn test_archive_before_moves_old_records() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        store.records.push(make_record(1, 1_000));
        store.records.push(make_record(2, 2_000));
        store.records.push(make_record(3, 3_000));

        let archived = store.archive_before(2_500);
        assert_eq!(archived, 2);
        assert_eq!(store.len(), 1);
        assert_eq!(store.archive_len(), 2);
    }

    #[test]
    fn test_archive_before_nothing_to_archive() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        store.records.push(make_record(1, 5_000));

        let archived = store.archive_before(1_000);
        assert_eq!(archived, 0);
        assert_eq!(store.len(), 1);
        assert_eq!(store.archive_len(), 0);
    }

    #[test]
    fn test_archive_export_paginates() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        for i in 0..15_u64 {
            store.archive.push(make_record(i, i));
        }

        let batch = store.archive_export(0, 10);
        assert_eq!(batch.records.len(), 10);
        assert!(batch.has_more);

        let tail = store.archive_export(batch.next_cursor, 10);
        assert_eq!(tail.records.len(), 5);
        assert!(!tail.has_more);
    }

    #[test]
    fn test_clear_archive() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        store.archive.push(make_record(1, 1));
        store.archive.push(make_record(2, 2));

        let cleared = store.clear_archive();
        assert_eq!(cleared, 2);
        assert_eq!(store.archive_len(), 0);
    }

    // ── get_by_id ────────────────────────────────────────────────────────────

    #[test]
    fn test_get_by_id_found() {
        let mut store = RequestRecordStore::new(RequestRetentionPolicy::default());
        store.records.push(make_record(42, 1_000));
        let rec = store.get_by_id(42).unwrap();
        assert_eq!(rec.id, 42);
    }

    #[test]
    fn test_get_by_id_not_found() {
        let store = RequestRecordStore::new(RequestRetentionPolicy::default());
        assert!(store.get_by_id(99).is_none());
    }
}

//! Each response type carries a [`SchemaVersion`] field so consumers can
//! distinguish which set of validation rules was applied.  The version-aware
//! constructors (`validate_deposit_with_version`, …) let callers request a
//! specific rule set; unknown versions fall back to the latest.

extern crate alloc;

use crate::errors::Error;

// ── Schema versioning ─────────────────────────────────────────────────────────

/// The current validator schema version.
pub const VALIDATOR_SCHEMA_V1: u32 = 1;

/// Semantic schema version for validator rule sets.
///
/// Each version corresponds to a specific set of validation rules.  When new
/// rules are added the version is bumped so that existing consumers can
/// explicitly opt in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SchemaVersion(pub u32);

impl SchemaVersion {
    /// Schema version 1 (initial set of rules).
    pub const V1: SchemaVersion = SchemaVersion(1);

    /// The latest known schema version – used as the default when no version
    /// is explicitly requested.
    pub const LATEST: SchemaVersion = SchemaVersion::V1;

    /// Resolve a raw `u32` to the corresponding [`SchemaVersion`].
    ///
    /// Unknown (future) versions are silently downgraded to [`LATEST`] so
    /// that consumers that blindly forward a version never receive a hard
    /// failure.
    pub fn resolve(v: u32) -> SchemaVersion {
        match v {
            1 => SchemaVersion::V1,
            _ => SchemaVersion::LATEST,
        }
    }
}

impl Default for SchemaVersion {
    fn default() -> Self {
        SchemaVersion::LATEST
    }
}

// ── Response types ────────────────────────────────────────────────────────────

/// A validated deposit response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositResponse {
    pub transaction_id: alloc::string::String,
    pub status: alloc::string::String,
    pub deposit_address: alloc::string::String,
    pub expires_at: u64,
    /// Schema version used to validate this response.
    pub schema_version: SchemaVersion,
}

/// A validated withdraw response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawResponse {
    pub transaction_id: alloc::string::String,
    pub status: alloc::string::String,
    pub estimated_completion: u64,
    /// Schema version used to validate this response.
    pub schema_version: SchemaVersion,
}

/// A validated quote response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuoteResponse {
    pub id: alloc::string::String,
    pub status: alloc::string::String,
    pub amount: u64,
    pub asset: alloc::string::String,
    pub fee: u64,
    /// Schema version used to validate this response.
    pub schema_version: SchemaVersion,
}

/// A validated SEP-38 quote response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sep38QuoteResponse {
    pub id: alloc::string::String,
    pub expires_at: alloc::string::String,
    pub price: alloc::string::String,
    pub sell_amount: alloc::string::String,
    pub buy_amount: alloc::string::String,
    pub fee: alloc::string::String,
    /// Schema version used to validate this response.
    pub schema_version: SchemaVersion,
}

/// A validated anchor info response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnchorInfoResponse {
    pub name: alloc::string::String,
    pub supported_assets: alloc::vec::Vec<alloc::string::String>,
    /// Schema version used to validate this response.
    pub schema_version: SchemaVersion,
}

/// A validated transaction status response.
#[derive(Clone, Debug, PartialEq)]
pub struct TransactionStatusResponseValidated {
    pub transaction_id: alloc::string::String,
    pub status: alloc::string::String,
    pub kind: alloc::string::String,
    /// Schema version used to validate this response.
    pub schema_version: SchemaVersion,
}

// ── Status validators ─────────────────────────────────────────────────────────

/// Coarse classification of a SEP-6 transaction `status` string.
///
/// The default arm of [`sep6_status_class`] is [`Sep6StatusClass::Unknown`]: a
/// status the current vocabulary does not recognise — for example one a newer
/// anchor introduces — is never optimistically treated as a completed or
/// otherwise successful operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sep6StatusClass {
    /// Terminal success: the transfer finished (`completed`).
    Completed,
    /// Terminal non-success: the transfer will not complete as requested
    /// (`refunded`, `expired`, `error`).
    Failed,
    /// Still in flight — waiting on the user, the anchor, or the network.
    Pending,
    /// Not a recognised SEP-6 status.
    Unknown,
}

/// Classify a SEP-6 transaction `status` string.
///
/// The input is normalized via [`TransactionStatus::from_str`](crate::TransactionStatus::from_str)
/// to ensure consistent trimming and case-folding across all status-parsing paths.
///
/// Recognised values map to [`Completed`](Sep6StatusClass::Completed),
/// [`Failed`](Sep6StatusClass::Failed) or [`Pending`](Sep6StatusClass::Pending);
/// every other value falls through to [`Unknown`](Sep6StatusClass::Unknown).
pub fn sep6_status_class(status: &str) -> Sep6StatusClass {
    use crate::TransactionStatus;
    
    let parsed = TransactionStatus::from_str(status);
    
    // TransactionStatus::Error is used both for the literal "error" status
    // and for unrecognized statuses. Check if the normalized input was "error".
    if matches!(parsed, TransactionStatus::Error) {
        let normalized = status.trim().to_ascii_lowercase();
        if normalized == "error" {
            return Sep6StatusClass::Failed;
        } else {
            return Sep6StatusClass::Unknown;
        }
    }
    
    match parsed {
        TransactionStatus::Completed => Sep6StatusClass::Completed,
        TransactionStatus::Refunded 
        | TransactionStatus::Expired 
        | TransactionStatus::NoMarket
        | TransactionStatus::TooSmall
        | TransactionStatus::TooLarge => Sep6StatusClass::Failed,
        TransactionStatus::Pending
        | TransactionStatus::Incomplete
        | TransactionStatus::PendingExternal
        | TransactionStatus::PendingAnchor
        | TransactionStatus::PendingTrust
        | TransactionStatus::PendingUser
        | TransactionStatus::PendingStellar
        | TransactionStatus::WaitingCustomerAction => Sep6StatusClass::Pending,
        TransactionStatus::Error => unreachable!("handled above"),
    }
}

/// Returns `true` when `status` is a recognised SEP-6 transaction status.
///
/// Equivalent to `sep6_status_class(status) != Sep6StatusClass::Unknown`: an
/// unrecognised status is rejected rather than assumed valid.
fn is_valid_sep6_status(status: &str) -> bool {
    !matches!(sep6_status_class(status), Sep6StatusClass::Unknown)
}

/// Returns `true` when `status` is a recognised quote status.
fn is_valid_quote_status(status: &str) -> bool {
    matches!(status, "quoted" | "pending" | "expired" | "error")
}

// ── Numeric helpers ───────────────────────────────────────────────────────────

fn is_valid_positive_decimal(s: &str) -> bool {
    s.parse::<f64>().map(|v| v > 0.0).unwrap_or(false)
}

// ── Raw response body guards (#829, #830) ─────────────────────────────────────

/// Maximum number of bytes of an untrusted response body echoed back inside an
/// error message.
///
/// Diagnostic context is capped at this small limit so a pathological multi-
/// megabyte body from an anchor cannot inflate log volume or error-string
/// allocations. Bodies at or below this length are included verbatim.
pub const MAX_ERROR_BODY_LEN: usize = 256;

/// Whether an endpoint's response body is required to contain JSON.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyRequirement {
    /// The endpoint must return a JSON body (e.g. `/deposit`, `/info`).
    Required,
    /// The endpoint may legitimately return an empty body (e.g. a `204`).
    Optional,
}

/// Truncate an untrusted body to [`MAX_ERROR_BODY_LEN`] bytes for safe
/// inclusion in an error message, appending an elision marker when bytes were
/// dropped. The cut is made on a UTF-8 character boundary so the result is
/// always valid text.
fn body_for_error(body: &str) -> alloc::string::String {
    if body.len() <= MAX_ERROR_BODY_LEN {
        return alloc::string::String::from(body);
    }
    let mut end = MAX_ERROR_BODY_LEN;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = alloc::string::String::from(&body[..end]);
    out.push_str("… (truncated)");
    out
}

/// Validate a raw anchor response body before any field-level parsing.
///
/// This is the JSON validation entry point: callers pass the untrusted body
/// string exactly as received from the anchor's HTTP response.
///
/// # Rules
///
/// - When `requirement` is [`BodyRequirement::Required`] an empty or
///   whitespace-only body fails immediately with a stable reason, so callers
///   get "response body is empty" instead of a generic parse failure and no
///   parsing work is attempted (#829).
/// - When `requirement` is [`BodyRequirement::Optional`] an empty body is
///   accepted unchanged, preserving optional-empty response behavior (#829).
/// - A non-empty body must look like a JSON object or array (its first
///   non-whitespace byte is `{` or `[`); otherwise only the first
///   [`MAX_ERROR_BODY_LEN`] bytes of the body appear in the error context
///   (#830).
/// - JSON-shaped bodies pass through untouched; full structural validation
///   remains the responsibility of the field-level validators.
pub fn validate_response_body(body: &str, requirement: BodyRequirement) -> Result<(), Error> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return match requirement {
            BodyRequirement::Required => Err(Error::validation_error(
                "response body is empty but a JSON body is required",
            )),
            BodyRequirement::Optional => Ok(()),
        };
    }
    
    // Check if body starts with JSON delimiters
    match trimmed.as_bytes()[0] {
        b'{' | b'[' => {
            // Parse the JSON to ensure it's well-formed
            if let Err(parse_err) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return Err(Error::validation_error(&alloc::format!(
                    "response body is malformed JSON: {}",
                    parse_err,
                )));
            }
            Ok(())
        }
        _ => Err(Error::validation_error(&alloc::format!(
            "response body is not JSON: {}",
            body_for_error(body),
        ))),
    }
}

// ── Validation functions (backward-compatible originals) ───────────────────────
// Each existing function delegates to its versioned counterpart with
// `SchemaVersion::LATEST`.

/// Validates a raw deposit response.
///
/// See [`validate_deposit_with_version`] for details.
pub fn validate_deposit_response(
    transaction_id: &str,
    status: &str,
    deposit_address: &str,
    expires_at: u64,
    now_unix: u64,
) -> Result<DepositResponse, Error> {
    validate_deposit_with_version(
        transaction_id,
        status,
        deposit_address,
        expires_at,
        now_unix,
        SchemaVersion::LATEST,
    )
}

/// Validates a raw withdraw response.
///
/// See [`validate_withdraw_with_version`] for details.
pub fn validate_withdraw_response(
    transaction_id: &str,
    status: &str,
    estimated_completion: u64,
) -> Result<WithdrawResponse, Error> {
    validate_withdraw_with_version(
        transaction_id,
        status,
        estimated_completion,
        SchemaVersion::LATEST,
    )
}

/// Validates a raw quote response.
///
/// See [`validate_quote_with_version`] for details.
pub fn validate_quote_response(
    id: &str,
    status: &str,
    amount: u64,
    asset: &str,
    fee: u64,
) -> Result<QuoteResponse, Error> {
    validate_quote_with_version(id, status, amount, asset, fee, SchemaVersion::LATEST)
}

/// Validates a raw SEP-38 quote response.
///
/// See [`validate_sep38_quote_with_version`] for details.
pub fn validate_sep38_quote_response(
    id: &str,
    expires_at: &str,
    price: &str,
    sell_amount: &str,
    buy_amount: &str,
    fee: &str,
) -> Result<Sep38QuoteResponse, Error> {
    validate_sep38_quote_with_version(
        id,
        expires_at,
        price,
        sell_amount,
        buy_amount,
        fee,
        SchemaVersion::LATEST,
    )
}

/// Validates a raw anchor info response.
///
/// See [`validate_anchor_info_with_version`] for details.
pub fn validate_anchor_info_response(
    name: &str,
    supported_assets: alloc::vec::Vec<alloc::string::String>,
) -> Result<AnchorInfoResponse, Error> {
    validate_anchor_info_with_version(name, supported_assets, SchemaVersion::LATEST)
}

/// Validates a raw transaction status response (legacy name).
pub fn validate_transaction_status_response(
    transaction_id: &str,
    status: &str,
    kind: &str,
) -> Result<TransactionStatusResponseValidated, Error> {
    validate_transaction_status_with_version(
        transaction_id,
        status,
        kind,
        SchemaVersion::LATEST,
    )
}

/// Validates a raw transaction status response (v2 name).
pub fn validate_transaction_status_response_v2(
    transaction_id: &str,
    status: &str,
    kind: &str,
) -> Result<TransactionStatusResponseValidated, Error> {
    validate_transaction_status_with_version(
        transaction_id,
        status,
        kind,
        SchemaVersion::LATEST,
    )
}

// ── Version-aware validation functions ────────────────────────────────────────

/// Like [`validate_deposit_response`] but accepts an explicit [`SchemaVersion`].
///
/// # Version-specific rules
///
/// | Version | Rules |
/// |---------|-------|
/// | V1      | Empty strings rejected; status must be a recognised SEP-6 value; `expires_at` must be 0 or in the future; `deposit_address` must be non-empty |
pub fn validate_deposit_with_version(
    transaction_id: &str,
    status: &str,
    deposit_address: &str,
    expires_at: u64,
    now_unix: u64,
    version: SchemaVersion,
) -> Result<DepositResponse, Error> {
    match version {
        _ => validate_deposit_v1(
            transaction_id,
            status,
            deposit_address,
            expires_at,
            now_unix,
        ),
    }
}

fn validate_deposit_v1(
    transaction_id: &str,
    status: &str,
    deposit_address: &str,
    expires_at: u64,
    now_unix: u64,
) -> Result<DepositResponse, Error> {
    if transaction_id.is_empty() {
        return Err(Error::validation_error("transaction_id is empty"));
    }
    if status.is_empty() {
        return Err(Error::validation_error("status is empty"));
    }
    if !is_valid_sep6_status(status) {
        return Err(Error::validation_error("status is not a recognised SEP-6 value"));
    }
    if deposit_address.is_empty() {
        return Err(Error::validation_error("deposit_address is empty"));
    }
    if expires_at != 0 && expires_at <= now_unix {
        return Err(Error::validation_error("expires_at is in the past"));
    }

    Ok(DepositResponse {
        transaction_id: alloc::string::String::from(transaction_id),
        status: alloc::string::String::from(status),
        deposit_address: alloc::string::String::from(deposit_address),
        expires_at,
        schema_version: SchemaVersion::V1,
    })
}

/// Like [`validate_withdraw_response`] but accepts an explicit [`SchemaVersion`].
pub fn validate_withdraw_with_version(
    transaction_id: &str,
    status: &str,
    estimated_completion: u64,
    version: SchemaVersion,
) -> Result<WithdrawResponse, Error> {
    match version {
        _ => validate_withdraw_v1(transaction_id, status, estimated_completion),
    }
}

fn validate_withdraw_v1(
    transaction_id: &str,
    status: &str,
    estimated_completion: u64,
) -> Result<WithdrawResponse, Error> {
    if transaction_id.is_empty() {
        return Err(Error::validation_error("transaction_id is empty"));
    }
    if status.is_empty() {
        return Err(Error::validation_error("status is empty"));
    }
    if !is_valid_sep6_status(status) {
        return Err(Error::validation_error("status is not a recognised SEP-6 value"));
    }

    Ok(WithdrawResponse {
        transaction_id: alloc::string::String::from(transaction_id),
        status: alloc::string::String::from(status),
        estimated_completion,
        schema_version: SchemaVersion::V1,
    })
}

/// Like [`validate_quote_response`] but accepts an explicit [`SchemaVersion`].
pub fn validate_quote_with_version(
    id: &str,
    status: &str,
    amount: u64,
    asset: &str,
    fee: u64,
    version: SchemaVersion,
) -> Result<QuoteResponse, Error> {
    match version {
        _ => validate_quote_v1(id, status, amount, asset, fee),
    }
}

fn validate_quote_v1(
    id: &str,
    status: &str,
    amount: u64,
    asset: &str,
    fee: u64,
) -> Result<QuoteResponse, Error> {
    if id.is_empty() {
        return Err(Error::validation_error("id is empty"));
    }
    if status.is_empty() {
        return Err(Error::validation_error("status is empty"));
    }
    if !is_valid_quote_status(status) {
        return Err(Error::validation_error("status is not a recognised quote status"));
    }
    if amount == 0 {
        return Err(Error::validation_error("amount must be greater than zero"));
    }
    if fee > amount {
        return Err(Error::validation_error("fee exceeds amount"));
    }
    if asset.is_empty() {
        return Err(Error::validation_error("asset is empty"));
    }
    validate_stellar_asset(asset)?;

    Ok(QuoteResponse {
        id: alloc::string::String::from(id),
        status: alloc::string::String::from(status),
        amount,
        asset: alloc::string::String::from(asset),
        fee,
        schema_version: SchemaVersion::V1,
    })
}

/// Like [`validate_sep38_quote_response`] but accepts an explicit [`SchemaVersion`].
pub fn validate_sep38_quote_with_version(
    id: &str,
    expires_at: &str,
    price: &str,
    sell_amount: &str,
    buy_amount: &str,
    fee: &str,
    version: SchemaVersion,
) -> Result<Sep38QuoteResponse, Error> {
    match version {
        _ => validate_sep38_quote_v1(id, expires_at, price, sell_amount, buy_amount, fee),
    }
}

fn validate_sep38_quote_v1(
    id: &str,
    expires_at: &str,
    price: &str,
    sell_amount: &str,
    buy_amount: &str,
    fee: &str,
) -> Result<Sep38QuoteResponse, Error> {
    if id.is_empty() {
        return Err(Error::validation_error("id is empty"));
    }
    if expires_at.is_empty() {
        return Err(Error::validation_error("expires_at is empty"));
    }
    if price.is_empty() {
        return Err(Error::validation_error("price is empty"));
    }
    if sell_amount.is_empty() {
        return Err(Error::validation_error("sell_amount is empty"));
    }
    if buy_amount.is_empty() {
        return Err(Error::validation_error("buy_amount is empty"));
    }
    if fee.is_empty() {
        return Err(Error::validation_error("fee is empty"));
    }
    if !is_valid_positive_decimal(price) {
        return Err(Error::validation_error("price must be a positive number"));
    }
    if !is_valid_positive_decimal(sell_amount) {
        return Err(Error::validation_error("sell_amount must be a positive number"));
    }
    if !is_valid_positive_decimal(buy_amount) {
        return Err(Error::validation_error("buy_amount must be a positive number"));
    }
    if !is_valid_positive_decimal(fee) {
        return Err(Error::validation_error("fee must be a positive number"));
    }

    Ok(Sep38QuoteResponse {
        id: alloc::string::String::from(id),
        expires_at: alloc::string::String::from(expires_at),
        price: alloc::string::String::from(price),
        sell_amount: alloc::string::String::from(sell_amount),
        buy_amount: alloc::string::String::from(buy_amount),
        fee: alloc::string::String::from(fee),
        schema_version: SchemaVersion::V1,
    })
}

/// Like [`validate_anchor_info_response`] but accepts an explicit [`SchemaVersion`].
pub fn validate_anchor_info_with_version(
    name: &str,
    supported_assets: alloc::vec::Vec<alloc::string::String>,
    version: SchemaVersion,
) -> Result<AnchorInfoResponse, Error> {
    match version {
        _ => validate_anchor_info_v1(name, supported_assets),
    }
}

fn validate_anchor_info_v1(
    name: &str,
    supported_assets: alloc::vec::Vec<alloc::string::String>,
) -> Result<AnchorInfoResponse, Error> {
    if name.is_empty() {
        return Err(Error::validation_error("name is empty"));
    }
    if name.len() > 100 {
        return Err(Error::validation_error("name must be 100 characters or fewer"));
    }
    if supported_assets.is_empty() {
        return Err(Error::validation_error("supported_assets is empty"));
    }
    for asset in &supported_assets {
        validate_stellar_asset(asset.as_str())?;
    }

    Ok(AnchorInfoResponse {
        name: alloc::string::String::from(name),
        supported_assets,
        schema_version: SchemaVersion::V1,
    })
}

/// Like [`validate_transaction_status_response`] but accepts an explicit
/// [`SchemaVersion`].
pub fn validate_transaction_status_with_version(
    transaction_id: &str,
    status: &str,
    kind: &str,
    version: SchemaVersion,
) -> Result<TransactionStatusResponseValidated, Error> {
    match version {
        _ => validate_transaction_status_v1(transaction_id, status, kind),
    }
}

fn validate_transaction_status_v1(
    transaction_id: &str,
    status: &str,
    kind: &str,
) -> Result<TransactionStatusResponseValidated, Error> {
    if transaction_id.is_empty() {
        return Err(Error::validation_error("transaction_id is empty"));
    }
    if status.is_empty() {
        return Err(Error::validation_error("status is empty"));
    }
    if !is_valid_sep6_status(status) {
        return Err(Error::validation_error("status is not a recognised SEP-6 value"));
    }
    if kind.is_empty() {
        return Err(Error::validation_error("kind is empty"));
    }

    Ok(TransactionStatusResponseValidated {
        transaction_id: alloc::string::String::from(transaction_id),
        status: alloc::string::String::from(status),
        kind: alloc::string::String::from(kind),
        schema_version: SchemaVersion::V1,
    })
}

// ── Stellar asset & account validation (unchanged) ────────────────────────────

fn decode_base32(input: &[u8]) -> Option<alloc::vec::Vec<u8>> {
    let mut buffer: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    let mut bits = 0u32;
    let mut value = 0u32;
    for &ch in input {
        let val = decode_base32_value(ch)?;
        value = (value << 5) | (val as u32);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            buffer.push(((value >> bits) & 0xFF) as u8);
        }
    }
    if bits != 0 {
        return None;
    }
    Some(buffer)
}

fn decode_base32_value(ch: u8) -> Option<u8> {
    match ch {
        b'A'..=b'Z' => Some(ch - b'A'),
        b'2'..=b'7' => Some(ch - b'2' + 26),
        _ => None,
    }
}

fn is_valid_stellar_account_char(c: char) -> bool {
    matches!(c, 'A'..='Z' | '2'..='7')
}

fn is_valid_stellar_strkey(account_id: &str) -> bool {
    const ACCOUNT_ID_VERSION_BYTE: u8 = 6 << 3;
    let decoded = match decode_base32(account_id.as_bytes()) {
        Some(bytes) => bytes,
        None => return false,
    };
    if decoded.len() != 35 {
        return false;
    }
    if decoded[0] != ACCOUNT_ID_VERSION_BYTE {
        return false;
    }
    let checksum = u16::from_le_bytes([decoded[33], decoded[34]]);
    crc16_xmodem(&decoded[..33]) == checksum
}

fn crc16_xmodem(input: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in input {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if (crc & 0x8000) != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Validate a Stellar asset identifier.
///
/// Accepts:
/// - `"native"` (XLM)
/// - `"CODE:ISSUER"` where CODE is 1–12 alphanumeric chars and ISSUER is a
///   valid 56-character Stellar account address (starts with `G`, passes
///   base32 decoding and CRC16-XMODEM checksum via [`validate_stellar_account_id`]).
pub fn validate_stellar_asset(asset: &str) -> Result<(), Error> {
    if asset == "native" {
        return Ok(());
    }
    let parts: alloc::vec::Vec<&str> = asset.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(Error::validation_error("asset must be 'native' or 'CODE:ISSUER'"));
    }
    let code = parts[0];
    let issuer = parts[1];
    if code.is_empty() || code.len() > 12 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::validation_error("asset code must be 1-12 alphanumeric characters"));
    }
    // Delegate issuer validation to the shared account-ID validator so that
    // length, prefix, character set, and CRC16-XMODEM checksum are all
    // checked consistently in one place.
    validate_stellar_account_id(issuer)
        .map_err(|_| Error::validation_error("asset issuer must be a valid Stellar account address"))
}

pub fn normalize_stellar_account_id(account_id: &str) -> Result<alloc::string::String, Error> {
    let trimmed = account_id.trim();
    if trimmed.is_empty() {
        return Err(Error::validation_error("account_id is empty"));
    }
    if trimmed.chars().any(|c| c.is_ascii_whitespace()) {
        return Err(Error::validation_error("account_id must not contain whitespace"));
    }
    let normalized = trimmed.to_ascii_uppercase();
    if normalized.len() != 56 {
        return Err(Error::validation_error("account_id must be 56 characters"));
    }
    if !normalized.starts_with('G') {
        return Err(Error::validation_error("account_id must start with G"));
    }
    if !normalized.chars().all(is_valid_stellar_account_char) {
        return Err(Error::validation_error("account_id contains invalid characters"));
    }
    if !is_valid_stellar_strkey(&normalized) {
        return Err(Error::validation_error("account_id checksum is invalid"));
    }
    Ok(alloc::string::String::from(normalized))
}

pub fn validate_stellar_account_id(account_id: &str) -> Result<(), Error> {
    normalize_stellar_account_id(account_id).map(|_| ())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

// ── Tests ─────────────────────────────────────────────────────────────────────

// ── Issue #661: Response shape compatibility checks for older anchors ──────────

/// Classification of how compatible a response from an anchor is with the
/// current schema expectations.
///
/// Compatibility is intentionally non-binary: an older anchor may omit
/// optional fields that were added in later schema iterations while still
/// providing all required fields.  This enum lets callers decide whether to
/// accept, warn, or reject a response rather than applying a hard pass/fail.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompatibilityLevel {
    /// All required **and** optional fields are present and valid.
    FullyCompatible,
    /// All required fields are present and valid, but one or more optional
    /// fields recognised by the current schema are absent.  The response is
    /// usable but callers may miss enriched data.
    PartiallyCompatible,
    /// One or more required fields are missing or invalid.  The response
    /// cannot be used safely.
    Incompatible,
}

impl CompatibilityLevel {
    /// Returns a human-readable label for this compatibility level.
    pub fn label(&self) -> &'static str {
        match self {
            CompatibilityLevel::FullyCompatible   => "fully_compatible",
            CompatibilityLevel::PartiallyCompatible => "partially_compatible",
            CompatibilityLevel::Incompatible        => "incompatible",
        }
    }

    /// Returns `true` when the response is safe to use (required fields are
    /// all present), regardless of whether optional fields are missing.
    pub fn is_usable(&self) -> bool {
        matches!(self, CompatibilityLevel::FullyCompatible | CompatibilityLevel::PartiallyCompatible)
    }
}

/// Result of a compatibility check, pairing the level with a human-readable
/// reason and the list of optional fields that were absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// The overall compatibility classification.
    pub level: CompatibilityLevel,
    /// Short explanation of why this level was assigned.
    pub reason: alloc::string::String,
    /// Names of optional fields that are absent in this response.
    pub missing_optional_fields: alloc::vec::Vec<alloc::string::String>,
}

impl CompatibilityReport {
    /// Convenience: whether the report's [`CompatibilityLevel`] is usable.
    pub fn is_usable(&self) -> bool {
        self.level.is_usable()
    }

    fn fully_compatible() -> Self {
        CompatibilityReport {
            level: CompatibilityLevel::FullyCompatible,
            reason: alloc::string::String::from("all required and optional fields present"),
            missing_optional_fields: alloc::vec::Vec::new(),
        }
    }

    fn partially_compatible(missing: alloc::vec::Vec<alloc::string::String>) -> Self {
        CompatibilityReport {
            level: CompatibilityLevel::PartiallyCompatible,
            reason: alloc::string::String::from("required fields present; optional fields missing"),
            missing_optional_fields: missing,
        }
    }

    fn incompatible(reason: &str) -> Self {
        CompatibilityReport {
            level: CompatibilityLevel::Incompatible,
            reason: alloc::string::String::from(reason),
            missing_optional_fields: alloc::vec::Vec::new(),
        }
    }
}

// ── Deposit compatibility ──────────────────────────────────────────────────

/// Check compatibility of a raw deposit response from an older anchor.
///
/// Required fields: `transaction_id`, `status`, `deposit_address`.
/// Optional fields (recognised by the current schema): `expires_at`.
///
/// | transaction_id | status | deposit_address | expires_at | Result |
/// |---|---|---|---|---|
/// | present & valid | valid SEP-6 status | non-empty | any | FullyCompatible or PartiallyCompatible |
/// | empty            | –                 | –          | –   | Incompatible |
/// | present          | empty / invalid   | –          | –   | Incompatible |
/// | present          | valid             | empty      | –   | Incompatible |
pub fn check_deposit_compatibility(
    transaction_id: &str,
    status: &str,
    deposit_address: &str,
    expires_at: Option<u64>,
) -> CompatibilityReport {
    if transaction_id.is_empty() {
        return CompatibilityReport::incompatible("transaction_id is missing");
    }
    if status.is_empty() || !is_valid_sep6_status(status) {
        return CompatibilityReport::incompatible("status is missing or not a recognised SEP-6 value");
    }
    if deposit_address.is_empty() {
        return CompatibilityReport::incompatible("deposit_address is missing");
    }

    if expires_at.is_none() {
        CompatibilityReport::partially_compatible(alloc::vec![alloc::string::String::from("expires_at")])
    } else {
        CompatibilityReport::fully_compatible()
    }
}

// ── Withdraw compatibility ─────────────────────────────────────────────────

/// Check compatibility of a raw withdrawal response from an older anchor.
///
/// Required fields: `transaction_id`, `status`.
/// Optional fields: `estimated_completion`.
pub fn check_withdraw_compatibility(
    transaction_id: &str,
    status: &str,
    estimated_completion: Option<u64>,
) -> CompatibilityReport {
    if transaction_id.is_empty() {
        return CompatibilityReport::incompatible("transaction_id is missing");
    }
    if status.is_empty() || !is_valid_sep6_status(status) {
        return CompatibilityReport::incompatible("status is missing or not a recognised SEP-6 value");
    }

    if estimated_completion.is_none() {
        CompatibilityReport::partially_compatible(
            alloc::vec![alloc::string::String::from("estimated_completion")],
        )
    } else {
        CompatibilityReport::fully_compatible()
    }
}

// ── SEP-38 quote compatibility ─────────────────────────────────────────────

/// Check compatibility of a raw SEP-38 quote response from an older anchor.
///
/// Required fields: `id`, `price`, `sell_amount`, `buy_amount`.
/// Optional fields: `expires_at`, `fee`.
pub fn check_sep38_quote_compatibility(
    id: &str,
    price: &str,
    sell_amount: &str,
    buy_amount: &str,
    expires_at: Option<&str>,
    fee: Option<&str>,
) -> CompatibilityReport {
    if id.is_empty() {
        return CompatibilityReport::incompatible("id is missing");
    }
    if price.is_empty() || !is_valid_positive_decimal(price) {
        return CompatibilityReport::incompatible(
            "price is missing or not a positive number",
        );
    }
    if sell_amount.is_empty() || !is_valid_positive_decimal(sell_amount) {
        return CompatibilityReport::incompatible(
            "sell_amount is missing or not a positive number",
        );
    }
    if buy_amount.is_empty() || !is_valid_positive_decimal(buy_amount) {
        return CompatibilityReport::incompatible(
            "buy_amount is missing or not a positive number",
        );
    }

    let mut missing: alloc::vec::Vec<alloc::string::String> = alloc::vec::Vec::new();
    if expires_at.map(|s| s.is_empty()).unwrap_or(true) {
        missing.push(alloc::string::String::from("expires_at"));
    }
    if fee.map(|s| s.is_empty()).unwrap_or(true) {
        missing.push(alloc::string::String::from("fee"));
    }

    if missing.is_empty() {
        CompatibilityReport::fully_compatible()
    } else {
        CompatibilityReport::partially_compatible(missing)
    }
}

// ── Anchor info compatibility ──────────────────────────────────────────────

/// Check compatibility of a raw anchor info response from an older anchor.
///
/// Required fields: `name`, `supported_assets` (non-empty).
/// Optional fields: none currently defined — this is reserved for future
/// schema additions (e.g. `contact_email`, `documentation_url`).
pub fn check_anchor_info_compatibility(
    name: &str,
    supported_assets: &[alloc::string::String],
) -> CompatibilityReport {
    if name.is_empty() {
        return CompatibilityReport::incompatible("name is missing");
    }
    if supported_assets.is_empty() {
        return CompatibilityReport::incompatible("supported_assets is missing or empty");
    }
    CompatibilityReport::fully_compatible()
}

// ── Transaction status compatibility ──────────────────────────────────────

/// Check compatibility of a raw transaction status response from an older anchor.
///
/// Required fields: `transaction_id`, `status`, `kind`.
/// Optional fields: none currently defined.
pub fn check_transaction_status_compatibility(
    transaction_id: &str,
    status: &str,
    kind: &str,
) -> CompatibilityReport {
    if transaction_id.is_empty() {
        return CompatibilityReport::incompatible("transaction_id is missing");
    }
    if status.is_empty() || !is_valid_sep6_status(status) {
        return CompatibilityReport::incompatible(
            "status is missing or not a recognised SEP-6 value",
        );
    }
    if kind.is_empty() {
        return CompatibilityReport::incompatible("kind is missing");
    }
    CompatibilityReport::fully_compatible()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SchemaVersion ──────────────────────────────────────────────────────────

    #[test]
    fn test_schema_version_resolve_v1() {
        assert_eq!(SchemaVersion::resolve(1), SchemaVersion::V1);
    }

    #[test]
    fn test_schema_version_resolve_unknown_falls_back() {
        assert_eq!(SchemaVersion::resolve(99), SchemaVersion::LATEST);
        assert_eq!(SchemaVersion::resolve(0), SchemaVersion::LATEST);
    }

    #[test]
    fn test_schema_version_default_is_latest() {
        let v: SchemaVersion = Default::default();
        assert_eq!(v, SchemaVersion::LATEST);
    }

    #[test]
    fn test_schema_version_constants() {
        assert_eq!(VALIDATOR_SCHEMA_V1, 1);
        assert_eq!(SchemaVersion::V1.0, 1);
    }

    // ── Deposit validation ─────────────────────────────────────────────────────

    #[test]
    fn test_valid_deposit_response() {
        let result = validate_deposit_response("dep_123", "pending", "GDEPOSIT...", 2_000_000_000, 1_700_000_000);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.transaction_id, "dep_123");
        assert_eq!(r.status, "pending");
        assert_eq!(r.deposit_address, "GDEPOSIT...");
        assert_eq!(r.expires_at, 2_000_000_000);
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_deposit_missing_transaction_id() {
        let e = validate_deposit_response("", "pending", "GDEPOSIT...", 2_000_000_000, 1_700_000_000)
            .unwrap_err();
        assert_eq!(e.code, crate::errors::ErrorCode::ValidationError);
        assert!(e.context.as_deref().unwrap_or("").contains("transaction_id"));
    }

    #[test]
    fn test_deposit_missing_status() {
        let e = validate_deposit_response("dep_123", "", "GDEPOSIT...", 2_000_000_000, 1_700_000_000)
            .unwrap_err();
        assert_eq!(e.code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn test_deposit_invalid_status() {
        let e = validate_deposit_response("dep_123", "garbage_status", "GDEPOSIT...", 2_000_000_000, 1_700_000_000)
            .unwrap_err();
        assert_eq!(e.code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn test_deposit_valid_status_accepted() {
        for status in &[
            "pending_external", "pending_anchor", "pending_trust", "pending_user",
            "pending_user_transfer_start", "pending_user_transfer_complete", "completed",
            "refunded", "expired", "incomplete", "pending", "no_market", "too_small",
            "too_large", "pending_stellar", "waiting_customer_action", "error",
        ] {
            let result = validate_deposit_response("dep_1", status, "GADDR...", 0, 1_000);
            assert!(result.is_ok(), "expected OK for status '{}'", status);
        }
    }

    #[test]
    fn test_deposit_missing_deposit_address() {
        let result = validate_deposit_response("dep_123", "pending", "", 2_000_000_000, 1_700_000_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_deposit_zero_expires_at_is_valid() {
        let result = validate_deposit_response("dep_123", "pending", "GDEPOSIT...", 0, 1_700_000_000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_deposit_expires_at_in_past_rejected() {
        let result = validate_deposit_response("dep_123", "pending", "GDEPOSIT...", 1_000_000_000, 2_000_000_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_deposit_with_version_explicit() {
        let r = validate_deposit_with_version("d1", "completed", "G...", 0, 0, SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
        let r = validate_deposit_with_version("d1", "completed", "G...", 0, 0, SchemaVersion::resolve(99)).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_deposit_whitespace_status_rejected() {
        let result = validate_deposit_response("dep_1", "  ", "GADDR...", 0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_deposit_long_transaction_id_accepted() {
        let long_id = alloc::string::ToString::to_string("dep_") + &"x".repeat(200);
        let result = validate_deposit_response(&long_id, "pending", "GADDR...", 0, 0);
        assert!(result.is_ok());
    }

    // ── Withdraw validation ────────────────────────────────────────────────────

    #[test]
    fn test_valid_withdraw_response() {
        let result = validate_withdraw_response("wd_456", "completed", 2000);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.transaction_id, "wd_456");
        assert_eq!(r.status, "completed");
        assert_eq!(r.estimated_completion, 2000);
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_withdraw_missing_transaction_id() {
        let result = validate_withdraw_response("", "completed", 2000);
        assert!(result.is_err());
    }

    #[test]
    fn test_withdraw_missing_status() {
        let result = validate_withdraw_response("wd_456", "", 2000);
        assert!(result.is_err());
    }

    #[test]
    fn test_withdraw_invalid_status_rejected() {
        let result = validate_withdraw_response("wd_456", "not_a_real_status", 2000);
        assert!(result.is_err());
    }

    #[test]
    fn test_withdraw_status_validated() {
        for status in &["completed", "pending_external", "pending_anchor", "refunded", "expired", "error"] {
            let result = validate_withdraw_response("wd_1", status, 0);
            assert!(result.is_ok(), "expected OK for status '{}'", status);
        }
    }

    #[test]
    fn test_withdraw_estimated_completion_zero_accepted() {
        let result = validate_withdraw_response("wd_1", "completed", 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_withdraw_with_version_explicit() {
        let r = validate_withdraw_with_version("w1", "completed", 0, SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    // ── Quote validation ──────────────────────────────────────────────────────

    #[test]
    fn test_valid_quote_response() {
        let result = validate_quote_response(
            "quote_789", "quoted", 100_0000000,
            "USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5",
            500000,
        );
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.id, "quote_789");
        assert_eq!(r.status, "quoted");
        assert_eq!(r.amount, 100_0000000);
        assert_eq!(r.fee, 500000);
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_quote_valid_native_asset() {
        let result = validate_quote_response("q1", "quoted", 100, "native", 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quote_missing_id() {
        let result = validate_quote_response("", "quoted", 100, "native", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_missing_status() {
        let result = validate_quote_response("q1", "", 100, "native", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_missing_asset() {
        let result = validate_quote_response("q1", "quoted", 100, "", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_zero_amount_rejected() {
        let result = validate_quote_response("q1", "quoted", 0, "native", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_invalid_asset_rejected() {
        let result = validate_quote_response("q1", "quoted", 100, "BADFORMAT", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_invalid_status_rejected() {
        let result = validate_quote_response("q1", "unknown_status", 100, "native", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_all_valid_statuses() {
        for status in &["quoted", "pending", "expired", "error"] {
            let result = validate_quote_response("q1", status, 100, "native", 0);
            assert!(result.is_ok(), "expected OK for status '{}'", status);
        }
    }

    #[test]
    fn test_quote_fee_exceeds_amount_rejected() {
        let result = validate_quote_response("q1", "quoted", 100, "native", 200);
        assert!(result.is_err());
        let e = result.unwrap_err();
        assert!(e.context.as_deref().unwrap_or("").contains("fee"));
    }

    #[test]
    fn test_quote_fee_equals_amount_rejected() {
        let result = validate_quote_response("q1", "quoted", 100, "native", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_quote_fee_less_than_amount_accepted() {
        let result = validate_quote_response("q1", "quoted", 100, "native", 50);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quote_with_version_explicit() {
        let r = validate_quote_with_version("q1", "quoted", 100, "native", 0, SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    // ── SEP-38 quote validation ───────────────────────────────────────────────

    #[test]
    fn test_valid_sep38_quote_response() {
        let result = validate_sep38_quote_response(
            "quote_123",
            "2023-11-01T00:00:00Z",
            "1.05",
            "100.00",
            "105.00",
            "1.00",
        );
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.id, "quote_123");
        assert_eq!(r.expires_at, "2023-11-01T00:00:00Z");
        assert_eq!(r.price, "1.05");
        assert_eq!(r.sell_amount, "100.00");
        assert_eq!(r.buy_amount, "105.00");
        assert_eq!(r.fee, "1.00");
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_sep38_quote_missing_fields() {
        let ok = |id, exp, price, sell, buy, fee| {
            validate_sep38_quote_response(id, exp, price, sell, buy, fee).is_err()
        };
        assert!(ok("", "t", "1", "1", "1", "1"));
        assert!(ok("id", "", "1", "1", "1", "1"));
        assert!(ok("id", "t", "", "1", "1", "1"));
        assert!(ok("id", "t", "1", "", "1", "1"));
        assert!(ok("id", "t", "1", "1", "", "1"));
        assert!(ok("id", "t", "1", "1", "1", ""));
    }

    #[test]
    fn test_sep38_quote_numeric_validation() {
        assert!(validate_sep38_quote_response("id", "t", "0", "1", "1", "1").is_err());
        assert!(validate_sep38_quote_response("id", "t", "-1", "1", "1", "1").is_err());
        assert!(validate_sep38_quote_response("id", "t", "abc", "1", "1", "1").is_err());
        assert!(validate_sep38_quote_response("id", "t", "1", "0", "1", "1").is_err());
        assert!(validate_sep38_quote_response("id", "t", "1", "1", "0", "1").is_err());
        assert!(validate_sep38_quote_response("id", "t", "1", "1", "1", "0").is_err());
    }

    #[test]
    fn test_sep38_quote_with_version_explicit() {
        let r = validate_sep38_quote_with_version("id", "t", "1.0", "1.0", "1.0", "0.1", SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    // ── Anchor info validation ────────────────────────────────────────────────

    #[test]
    fn test_valid_anchor_info_response() {
        let assets = alloc::vec![
            alloc::string::String::from("native"),
            alloc::string::String::from("USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"),
        ];
        let result = validate_anchor_info_response("MyAnchor", assets);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.name, "MyAnchor");
        assert_eq!(r.supported_assets.len(), 2);
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_anchor_info_missing_name() {
        let assets = alloc::vec![alloc::string::String::from("native")];
        let result = validate_anchor_info_response("", assets);
        assert!(result.is_err());
    }

    #[test]
    fn test_anchor_info_empty_assets() {
        let result = validate_anchor_info_response("MyAnchor", alloc::vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_anchor_info_name_too_long() {
        let name = "A".repeat(101);
        let assets = alloc::vec![alloc::string::String::from("native")];
        let result = validate_anchor_info_response(&name, assets);
        assert!(result.is_err());
    }

    #[test]
    fn test_anchor_info_name_max_length_ok() {
        let name = "A".repeat(100);
        let assets = alloc::vec![alloc::string::String::from("native")];
        let result = validate_anchor_info_response(&name, assets);
        assert!(result.is_ok());
    }

    #[test]
    fn test_anchor_info_invalid_asset_identifier() {
        let assets = alloc::vec![alloc::string::String::from("NOTVALID")];
        let result = validate_anchor_info_response("MyAnchor", assets);
        assert!(result.is_err());
    }

    #[test]
    fn test_anchor_info_duplicate_assets_accepted() {
        let assets = alloc::vec![
            alloc::string::String::from("native"),
            alloc::string::String::from("native"),
        ];
        let result = validate_anchor_info_response("MyAnchor", assets);
        assert!(result.is_ok());
    }

    #[test]
    fn test_anchor_info_with_version_explicit() {
        let assets = alloc::vec![alloc::string::String::from("native")];
        let r = validate_anchor_info_with_version("A", assets, SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    // ── Transaction status validation ─────────────────────────────────────────

    #[test]
    fn test_valid_transaction_status_v1() {
        let result = validate_transaction_status_response("tx_123", "completed", "deposit");
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.transaction_id, "tx_123");
        assert_eq!(r.status, "completed");
        assert_eq!(r.kind, "deposit");
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_transaction_status_missing_fields() {
        assert!(validate_transaction_status_response("", "completed", "deposit").is_err());
        assert!(validate_transaction_status_response("tx_123", "", "deposit").is_err());
        assert!(validate_transaction_status_response("tx_123", "completed", "").is_err());
    }

    #[test]
    fn test_transaction_status_invalid_status_rejected() {
        let e = validate_transaction_status_response("tx_1", "garbage", "deposit").unwrap_err();
        assert_eq!(e.code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn test_transaction_status_all_valid_statuses() {
        for status in &[
            "pending_external", "pending_anchor", "pending_trust", "pending_user",
            "pending_user_transfer_start", "pending_user_transfer_complete", "completed",
            "refunded", "expired", "incomplete", "pending", "no_market", "too_small",
            "too_large", "pending_stellar", "waiting_customer_action", "error",
        ] {
            let result = validate_transaction_status_response("tx_1", status, "deposit");
            assert!(result.is_ok(), "expected OK for status '{}'", status);
        }
    }

    #[test]
    fn test_transaction_status_v2_alias() {
        let r1 = validate_transaction_status_response("tx_1", "completed", "deposit").unwrap();
        let r2 = validate_transaction_status_response_v2("tx_1", "completed", "deposit").unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_transaction_status_with_version_explicit() {
        let r = validate_transaction_status_with_version("t1", "completed", "deposit", SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    // ── Stellar asset validation ─────────────────────────────────────────────

    #[test]
    fn test_stellar_asset_native() {
        assert!(validate_stellar_asset("native").is_ok());
    }

    #[test]
    fn test_stellar_asset_valid_issued() {
        assert!(validate_stellar_asset(
            "USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
        ).is_ok());
    }

    #[test]
    fn test_stellar_asset_empty() {
        assert!(validate_stellar_asset("").is_err());
    }

    #[test]
    fn test_stellar_asset_no_colon() {
        assert!(validate_stellar_asset("USDC").is_err());
    }

    #[test]
    fn test_stellar_asset_code_too_long() {
        assert!(validate_stellar_asset(
            "TOOLONGCODE123:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
        ).is_err());
    }

    #[test]
    fn test_stellar_asset_issuer_wrong_length() {
        assert!(validate_stellar_asset("USDC:GSHORT").is_err());
    }

    #[test]
    fn test_stellar_asset_issuer_wrong_prefix() {
        assert!(validate_stellar_asset(
            "USDC:ABBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
        ).is_err());
    }

    #[test]
    fn test_stellar_asset_issuer_bad_checksum_rejected() {
        // Valid address with the last character mutated (checksum corruption).
        // GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5 is valid;
        // replacing the trailing '5' with '6' produces a different checksum.
        assert!(validate_stellar_asset(
            "USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA6"
        ).is_err(), "asset with checksum-corrupted issuer must be rejected");
    }

    // ── Stellar account validation ───────────────────────────────────────────

    #[test]
    fn test_stellar_account_id_valid() {
        assert!(normalize_stellar_account_id(
            "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
        ).is_ok());
    }

    #[test]
    fn test_stellar_account_id_normalizes_lowercase_and_whitespace() {
        let normalized = normalize_stellar_account_id(
            "  gbbd47if6lwk7p7mdevscwr7dpuwv3ny3dtqevfl4nat4aqh3zllfla5  "
        ).unwrap();
        assert_eq!(normalized, "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5");
    }

    #[test]
    fn test_stellar_account_id_wrong_length() {
        assert!(validate_stellar_account_id("GBBD47IF6LWK7P7MDEVS").is_err());
    }

    #[test]
    fn test_stellar_account_id_wrong_prefix() {
        assert!(validate_stellar_account_id(
            "ABBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
        ).is_err());
    }

    #[test]
    fn test_stellar_account_id_invalid_checksum() {
        assert!(validate_stellar_account_id(
            "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA6"
        ).is_err());
    }

    #[test]
    fn test_stellar_account_id_invalid_character() {
        assert!(validate_stellar_account_id(
            "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFL!5"
        ).is_err());
    }

    // ── Error messages are descriptive ───────────────────────────────────────

    #[test]
    fn test_error_message_contains_field_name() {
        let e = validate_deposit_response("", "pending", "G...", 0, 0).unwrap_err();
        assert!(e.context.as_deref().unwrap_or("").contains("transaction_id"));

        let e = validate_deposit_response("d1", "", "G...", 0, 0).unwrap_err();
        assert!(e.context.as_deref().unwrap_or("").contains("status"));

        let e = validate_quote_response("", "quoted", 100, "native", 0).unwrap_err();
        assert!(e.context.as_deref().unwrap_or("").contains("id"));
    }

    #[test]
    fn test_validation_error_does_not_panic() {
        let result = validate_deposit_response("", "", "", 0, 0);
        match result {
            Err(e) if e.code == crate::errors::ErrorCode::ValidationError => {}
            _ => panic!("Expected ValidationError"),
        }
    }

    // ── Schema version round-trip ─────────────────────────────────────────────

    #[test]
    fn test_deposit_schema_version_preserved() {
        for v in &[SchemaVersion::V1] {
            let r = validate_deposit_with_version("d1", "completed", "G...", 0, 0, *v).unwrap();
            assert_eq!(r.schema_version, *v);
        }
    }

    #[test]
    fn test_withdraw_schema_version_preserved() {
        let r = validate_withdraw_with_version("w1", "completed", 0, SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_quote_schema_version_preserved() {
        let r = validate_quote_with_version("q1", "quoted", 100, "native", 0, SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_sep38_quote_schema_version_preserved() {
        let r = validate_sep38_quote_with_version("id", "t", "1.0", "1.0", "1.0", "0.1", SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    #[test]
    fn test_anchor_info_schema_version_preserved() {
        let assets = alloc::vec![alloc::string::String::from("native")];
        let r = validate_anchor_info_with_version("A", assets, SchemaVersion::V1).unwrap();
        assert_eq!(r.schema_version, SchemaVersion::V1);
    }

    // ── Issue #661: compatibility check tests ─────────────────────────────────

    // -- CompatibilityLevel helpers --

    #[test]
    fn test_compatibility_level_is_usable() {
        assert!(CompatibilityLevel::FullyCompatible.is_usable());
        assert!(CompatibilityLevel::PartiallyCompatible.is_usable());
        assert!(!CompatibilityLevel::Incompatible.is_usable());
    }

    #[test]
    fn test_compatibility_level_labels() {
        assert_eq!(CompatibilityLevel::FullyCompatible.label(),    "fully_compatible");
        assert_eq!(CompatibilityLevel::PartiallyCompatible.label(), "partially_compatible");
        assert_eq!(CompatibilityLevel::Incompatible.label(),        "incompatible");
    }

    // -- Deposit compatibility --

    #[test]
    fn test_deposit_compat_fully_compatible() {
        let r = check_deposit_compatibility("txn-1", "completed", "GABC...", Some(9999999));
        assert_eq!(r.level, CompatibilityLevel::FullyCompatible);
        assert!(r.is_usable());
        assert!(r.missing_optional_fields.is_empty());
    }

    #[test]
    fn test_deposit_compat_partially_compatible_missing_expires_at() {
        let r = check_deposit_compatibility("txn-1", "pending", "GABC...", None);
        assert_eq!(r.level, CompatibilityLevel::PartiallyCompatible);
        assert!(r.is_usable());
        assert!(r.missing_optional_fields.contains(&alloc::string::String::from("expires_at")));
    }

    #[test]
    fn test_deposit_compat_incompatible_missing_txn_id() {
        let r = check_deposit_compatibility("", "completed", "GABC...", None);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
        assert!(!r.is_usable());
    }

    #[test]
    fn test_deposit_compat_incompatible_invalid_status() {
        let r = check_deposit_compatibility("txn-1", "unknown_status", "GABC...", None);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn test_deposit_compat_incompatible_empty_address() {
        let r = check_deposit_compatibility("txn-1", "completed", "", None);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    // -- Withdraw compatibility --

    #[test]
    fn test_withdraw_compat_fully_compatible() {
        let r = check_withdraw_compatibility("txn-2", "completed", Some(1_700_000_000));
        assert_eq!(r.level, CompatibilityLevel::FullyCompatible);
        assert!(r.missing_optional_fields.is_empty());
    }

    #[test]
    fn test_withdraw_compat_partially_compatible_missing_estimated_completion() {
        let r = check_withdraw_compatibility("txn-2", "pending_external", None);
        assert_eq!(r.level, CompatibilityLevel::PartiallyCompatible);
        assert!(r.missing_optional_fields.contains(
            &alloc::string::String::from("estimated_completion")
        ));
    }

    #[test]
    fn test_withdraw_compat_incompatible_empty_status() {
        let r = check_withdraw_compatibility("txn-2", "", None);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn test_withdraw_compat_incompatible_missing_txn_id() {
        let r = check_withdraw_compatibility("", "completed", Some(0));
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    // -- SEP-38 quote compatibility --

    #[test]
    fn test_sep38_quote_compat_fully_compatible() {
        let r = check_sep38_quote_compatibility(
            "qid", "1.5", "100.0", "150.0",
            Some("2026-01-01T00:00:00Z"), Some("0.5"),
        );
        assert_eq!(r.level, CompatibilityLevel::FullyCompatible);
    }

    #[test]
    fn test_sep38_quote_compat_partially_compatible_missing_both_optional() {
        let r = check_sep38_quote_compatibility(
            "qid", "1.5", "100.0", "150.0", None, None,
        );
        assert_eq!(r.level, CompatibilityLevel::PartiallyCompatible);
        assert!(r.missing_optional_fields.contains(&alloc::string::String::from("expires_at")));
        assert!(r.missing_optional_fields.contains(&alloc::string::String::from("fee")));
    }

    #[test]
    fn test_sep38_quote_compat_partially_compatible_missing_fee_only() {
        let r = check_sep38_quote_compatibility(
            "qid", "1.5", "100.0", "150.0",
            Some("2026-01-01T00:00:00Z"), None,
        );
        assert_eq!(r.level, CompatibilityLevel::PartiallyCompatible);
        assert!(r.missing_optional_fields.contains(&alloc::string::String::from("fee")));
        assert!(!r.missing_optional_fields.contains(&alloc::string::String::from("expires_at")));
    }

    #[test]
    fn test_sep38_quote_compat_incompatible_empty_id() {
        let r = check_sep38_quote_compatibility("", "1.5", "100.0", "150.0", None, None);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn test_sep38_quote_compat_incompatible_invalid_price() {
        let r = check_sep38_quote_compatibility("qid", "not-a-number", "100.0", "150.0", None, None);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn test_sep38_quote_compat_incompatible_zero_sell_amount() {
        let r = check_sep38_quote_compatibility("qid", "1.5", "0.0", "150.0", None, None);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    // -- Anchor info compatibility --

    #[test]
    fn test_anchor_info_compat_fully_compatible() {
        let assets = alloc::vec![alloc::string::String::from("native")];
        let r = check_anchor_info_compatibility("Anchor Corp", &assets);
        assert_eq!(r.level, CompatibilityLevel::FullyCompatible);
    }

    #[test]
    fn test_anchor_info_compat_incompatible_empty_name() {
        let assets = alloc::vec![alloc::string::String::from("native")];
        let r = check_anchor_info_compatibility("", &assets);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn test_anchor_info_compat_incompatible_empty_assets() {
        let r = check_anchor_info_compatibility("Anchor Corp", &[]);
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    // -- Transaction status compatibility --

    #[test]
    fn test_tx_status_compat_fully_compatible() {
        let r = check_transaction_status_compatibility("txn-99", "completed", "deposit");
        assert_eq!(r.level, CompatibilityLevel::FullyCompatible);
    }

    #[test]
    fn test_tx_status_compat_incompatible_missing_kind() {
        let r = check_transaction_status_compatibility("txn-99", "completed", "");
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn test_tx_status_compat_incompatible_invalid_status() {
        let r = check_transaction_status_compatibility("txn-99", "bad_status", "deposit");
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn test_tx_status_compat_incompatible_empty_txn_id() {
        let r = check_transaction_status_compatibility("", "completed", "withdrawal");
        assert_eq!(r.level, CompatibilityLevel::Incompatible);
    }

    // ── Issue #831: unknown status must not be treated as a success ───────────

    #[test]
    fn test_sep6_status_class_covers_full_vocabulary() {
        // Every status is_valid_sep6_status accepts must classify as a concrete
        // (non-Unknown) class, and the terminal ones keep their meaning.
        for status in &[
            "pending_external", "pending_anchor", "pending_trust", "pending_user",
            "pending_user_transfer_start", "pending_user_transfer_complete", "completed",
            "refunded", "expired", "incomplete", "pending", "no_market", "too_small",
            "too_large", "pending_stellar", "waiting_customer_action", "error",
        ] {
            assert_ne!(sep6_status_class(status), Sep6StatusClass::Unknown, "{status}");
            assert!(is_valid_sep6_status(status), "'{status}' must stay recognised");
        }
        assert_eq!(sep6_status_class("completed"), Sep6StatusClass::Completed);
        assert_eq!(sep6_status_class("refunded"), Sep6StatusClass::Failed);
        assert_eq!(sep6_status_class("expired"), Sep6StatusClass::Failed);
        assert_eq!(sep6_status_class("error"), Sep6StatusClass::Failed);
        assert_eq!(sep6_status_class("pending_anchor"), Sep6StatusClass::Pending);
    }

    #[test]
    fn test_sep6_status_class_unknown_is_not_completed() {
        // A status a newer anchor might introduce must classify as Unknown,
        // never Completed, and must not pass validation.
        for unknown in ["pending_regulatory_review", "settled", ""] {
            assert_eq!(sep6_status_class(unknown), Sep6StatusClass::Unknown, "{unknown}");
            assert!(!is_valid_sep6_status(unknown), "{unknown}");
        }
    }

    #[test]
    fn test_sep6_status_class_normalizes_case_and_whitespace() {
        // Issue #1138: sep6_status_class must normalize input like TransactionStatus::from_str
        assert_eq!(sep6_status_class("COMPLETED"), Sep6StatusClass::Completed);
        assert_eq!(sep6_status_class("Completed"), Sep6StatusClass::Completed);
        assert_eq!(sep6_status_class("  completed  "), Sep6StatusClass::Completed);
        assert_eq!(sep6_status_class("  COMPLETED  "), Sep6StatusClass::Completed);
        
        assert_eq!(sep6_status_class("PENDING_EXTERNAL"), Sep6StatusClass::Pending);
        assert_eq!(sep6_status_class("  pending_anchor  "), Sep6StatusClass::Pending);
        assert_eq!(sep6_status_class("PeNdInG_uSeR"), Sep6StatusClass::Pending);
        
        assert_eq!(sep6_status_class("REFUNDED"), Sep6StatusClass::Failed);
        assert_eq!(sep6_status_class("  expired  "), Sep6StatusClass::Failed);
        assert_eq!(sep6_status_class("ErRoR"), Sep6StatusClass::Failed);
        
        // Normalized versions must also pass validation
        assert!(is_valid_sep6_status("COMPLETED"));
        assert!(is_valid_sep6_status("  pending_user  "));
        assert!(is_valid_sep6_status("Refunded"));
    }

    #[test]
    fn test_validators_reject_unknown_status_regression() {
        // Deposit / withdraw / transaction-status validators must all fail
        // closed on an unrecognised status rather than accept it as valid.
        let unknown = "pending_regulatory_review";
        assert!(validate_deposit_response("d1", unknown, "GADDR...", 0, 0).is_err());
        assert!(validate_withdraw_response("w1", unknown, 0).is_err());
        assert!(validate_transaction_status_response("t1", unknown, "deposit").is_err());
    }

    // ── Issue #829: reject empty response body where required ─────────────────

    #[test]
    fn test_required_body_rejects_empty_with_stable_reason() {
        let e = validate_response_body("", BodyRequirement::Required).unwrap_err();
        assert_eq!(e.code, crate::errors::ErrorCode::ValidationError);
        assert!(e.context.as_deref().unwrap_or("").contains("empty"));
    }

    #[test]
    fn test_required_body_rejects_whitespace_only() {
        assert!(validate_response_body("   \n\t ", BodyRequirement::Required).is_err());
    }

    #[test]
    fn test_optional_body_allows_empty() {
        assert!(validate_response_body("", BodyRequirement::Optional).is_ok());
        assert!(validate_response_body("   ", BodyRequirement::Optional).is_ok());
    }

    #[test]
    fn test_valid_json_body_passes_unchanged() {
        assert!(validate_response_body(r#"{"transaction_id":"x"}"#, BodyRequirement::Required).is_ok());
        assert!(validate_response_body("  [1,2,3]  ", BodyRequirement::Required).is_ok());
        assert!(validate_response_body(r#"{"a":1}"#, BodyRequirement::Optional).is_ok());
    }

    #[test]
    fn test_malformed_json_object_rejected() {
        let result = validate_response_body("{broken", BodyRequirement::Required);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, crate::errors::ErrorCode::ValidationError);
    }

    #[test]
    fn test_malformed_json_array_rejected() {
        let result = validate_response_body("[1,2,", BodyRequirement::Required);
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_json_with_trailing_comma() {
        let result = validate_response_body(r#"{"a":1,}"#, BodyRequirement::Required);
        assert!(result.is_err());
    }

    #[test]
    fn test_wellformed_json_accepted() {
        assert!(validate_response_body(r#"{"nested":{"a":1}}"#, BodyRequirement::Required).is_ok());
        assert!(validate_response_body(r#"[{"a":1},{"b":2}]"#, BodyRequirement::Required).is_ok());
    }

    // ── Issue #830: error context from an untrusted body is bounded ──────────

    #[test]
    fn test_error_body_is_truncated_to_limit() {
        let huge = "x".repeat(50_000); // not JSON-shaped
        let e = validate_response_body(&huge, BodyRequirement::Required).unwrap_err();
        let ctx = e.context.as_deref().unwrap_or("");
        assert!(ctx.len() < huge.len(), "error context must not grow with the body");
        // Fixed prefix + at most MAX_ERROR_BODY_LEN body bytes + elision marker.
        assert!(ctx.len() <= MAX_ERROR_BODY_LEN + 64);
        assert!(ctx.contains("truncated"));
    }

    #[test]
    fn test_error_body_short_diagnostic_unchanged() {
        let e = validate_response_body("nope", BodyRequirement::Required).unwrap_err();
        let ctx = e.context.as_deref().unwrap_or("");
        assert!(ctx.contains("nope"));
        assert!(!ctx.contains("truncated"));
    }

    #[test]
    fn test_body_for_error_respects_char_boundary() {
        let s = "é".repeat(400); // 2 bytes each
        let out = body_for_error(&s);
        assert!(out.starts_with('é'));
        assert!(out.len() <= MAX_ERROR_BODY_LEN + "… (truncated)".len());
        assert!(out.ends_with("… (truncated)"));
    }

    #[test]
    fn test_body_for_error_leaves_short_body_intact() {
        assert_eq!(body_for_error("{\"ok\":true}"), "{\"ok\":true}");
    }
}
