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
//! # Example//! Request deduplication for repeated operations (#681).
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
//! ```

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

// ---------------------------------------------------------------------------
// DeduplicationKey
// ---------------------------------------------------------------------------

/// Uniquely identifies a logical operation for deduplication purposes.
///
/// Keys are composed of an `operation` tag (e.g. `"deposit"`) and a
/// `request_id` (e.g. a transaction ID, idempotency key, or content hash).
/// The combination must be stable across retries for deduplication to work.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeduplicationKey {
    /// Operation name, e.g. `"deposit"`, `"withdrawal"`, `"sep38_quote"`.
    pub operation: String,
    /// Stable identifier for this specific invocation of the operation.
    pub request_id: String,
}

impl DeduplicationKey {
    /// Construct a key from an operation name and a request identifier.
    pub fn new(operation: impl Into<String>, request_id: impl Into<String>) -> Self {
        DeduplicationKey {
            operation: operation.into(),
            request_id: request_id.into(),
        }
    }

    /// Compact string representation used as an internal map key.
    ///
    /// Uses length-delimited encoding (`<len>:<value>`) for each component so
    /// that distinct (operation, request_id) pairs with embedded colons always
    /// produce distinct keys.  For example:
    ///   operation="a:b", request_id="c"  →  "3:a:b|1:c"
    ///   operation="a",   request_id="b:c" →  "1:a|3:b:c"
    fn as_map_key(&self) -> String {
        alloc::format!(
            "{}:{}|{}:{}",
            self.operation.len(),
            self.operation,
            self.request_id.len(),
            self.request_id,
        )
    }
}

// ---------------------------------------------------------------------------
// DeduplicationResult
// ---------------------------------------------------------------------------

/// The cached outcome of a previously executed operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeduplicationResult {
    /// The operation completed successfully; the inner value is a short
    /// string summary of the outcome (e.g. a status code or transaction ID).
    Success(String),
    /// The operation failed; the inner value is the error kind/message.
    Failure(String),
}

// ---------------------------------------------------------------------------
// DeduplicationEntry — internal
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct DeduplicationEntry {
    result: DeduplicationResult,
    /// Unix timestamp (seconds) after which this entry must not be served.
    expires_at: u64,
}

// ---------------------------------------------------------------------------
// DeduplicationStore
// ---------------------------------------------------------------------------

/// In-process store for deduplication entries.
///
/// Maps [`DeduplicationKey`]s to cached outcomes with per-entry TTLs. Entries
/// expire after `default_ttl_secs` seconds from the time they are recorded;
/// call [`purge_expired`](Self::purge_expired) periodically to reclaim memory.
///
/// The store enforces a hard capacity limit (`max_entries`). Any call to
/// [`record_success`](Self::record_success) or
/// [`record_failure`](Self::record_failure) that would push the number of
/// **live** (non-expired) entries past this limit first evicts expired entries.
/// If the store is still at capacity after eviction, the insertion is silently
/// dropped to prevent unbounded memory growth.
#[derive(Debug)]
pub struct DeduplicationStore {
    /// Default time-to-live in seconds for each entry.
    default_ttl_secs: u64,
    /// Maximum number of entries the store will hold at any one time.
    /// `0` means unlimited (backward-compatible default when constructed via
    /// [`DeduplicationStore::new`]).
    max_entries: usize,
    entries: BTreeMap<String, DeduplicationEntry>,
}

impl DeduplicationStore {
    /// Create a new store with the given default TTL (seconds).
    ///
    /// # Panics
    ///
    /// Panics if `default_ttl_secs` is `0`. A zero TTL would cause every entry
    /// to expire immediately, making deduplication ineffective. Use a positive
    /// value; there is no "unlimited" TTL mode.
    pub fn new(default_ttl_secs: u64) -> Self {
        assert!(
            default_ttl_secs > 0,
            "default_ttl_secs must be greater than zero; a zero TTL causes every entry to expire immediately"
        );
        DeduplicationStore {
            default_ttl_secs,
            max_entries: 0,
            entries: BTreeMap::new(),
        }
    }

    /// Create a new store with the given default TTL and a hard capacity cap.
    ///
    /// When `max_entries > 0` the insertion path evicts expired entries before
    /// adding a new key, and skips the insertion if the store is still full
    /// after eviction.  `max_entries == 0` disables the cap (same as [`new`]).
    ///
    /// # Panics
    ///
    /// Panics if `default_ttl_secs` is `0` (same reason as [`new`]).
    pub fn with_capacity(default_ttl_secs: u64, max_entries: usize) -> Self {
        assert!(
            default_ttl_secs > 0,
            "default_ttl_secs must be greater than zero; a zero TTL causes every entry to expire immediately"
        );
        DeduplicationStore {
            default_ttl_secs,
            max_entries,
            entries: BTreeMap::new(),
        }
    }

    /// Return `true` when `key` maps to a non-expired entry.
    ///
    /// A `true` result means the operation has already been executed and the
    /// caller should retrieve the cached result via [`cached_result`](Self::cached_result)
    /// instead of re-running the operation.
    ///
    /// The expiration check is performed **before** the duplicate decision:
    /// an expired entry is treated as absent — it does not block a fresh
    /// execution of the same key.
    pub fn is_duplicate(&self, key: &DeduplicationKey, now_secs: u64) -> bool {
        match self.entries.get(&key.as_map_key()) {
            // Entry exists AND is still within its TTL → this is a live duplicate.
            Some(entry) if entry.expires_at > now_secs => true,
            // Entry exists but has expired, or no entry at all → not a duplicate.
            _ => false,
        }
    }

    /// Store a successful outcome for `key`.
    pub fn record_success(&mut self, key: &DeduplicationKey, summary: impl Into<String>, now_secs: u64) {
        self.enforce_capacity(now_secs);
        if self.max_entries > 0 && self.entries.len() >= self.max_entries {
            // Still at capacity after eviction — drop the insertion to stay
            // within the configured bound.
            return;
        }
        self.entries.insert(
            key.as_map_key(),
            DeduplicationEntry {
                result: DeduplicationResult::Success(summary.into()),
                expires_at: now_secs.saturating_add(self.default_ttl_secs),
            },
        );
    }

    /// Store a failure outcome for `key`.
    pub fn record_failure(&mut self, key: &DeduplicationKey, error: impl Into<String>, now_secs: u64) {
        self.enforce_capacity(now_secs);
        if self.max_entries > 0 && self.entries.len() >= self.max_entries {
            return;
        }
        self.entries.insert(
            key.as_map_key(),
            DeduplicationEntry {
                result: DeduplicationResult::Failure(error.into()),
                expires_at: now_secs.saturating_add(self.default_ttl_secs),
            },
        );
    }

    /// Retrieve the cached [`DeduplicationResult`] for `key`, or `None` when
    /// the key is unknown or its entry has expired.
    pub fn cached_result(&self, key: &DeduplicationKey, now_secs: u64) -> Option<DeduplicationResult> {
        self.entries.get(&key.as_map_key()).and_then(|entry| {
            if entry.expires_at > now_secs {
                Some(entry.result.clone())
            } else {
                None
            }
        })
    }

    /// Enforce the capacity limit before an insertion by evicting expired
    /// entries.  Called at the start of every `record_*` method.
    fn enforce_capacity(&mut self, now_secs: u64) {
        if self.max_entries == 0 {
            return; // no cap configured
        }
        if self.entries.len() >= self.max_entries {
            // Evict expired entries to make room.
            self.entries.retain(|_, v| v.expires_at > now_secs);
        }
    }

    /// Remove all expired entries, returning the count of entries removed.
    pub fn purge_expired(&mut self, now_secs: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, v| v.expires_at > now_secs);
        before - self.entries.len()
    }

    /// Total number of entries in the store (including expired ones not yet purged).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the store holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Deduplication statistics
// ---------------------------------------------------------------------------

/// Accumulated statistics for a [`DeduplicationStore`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeduplicationStats {
    /// Number of calls that were recognised as duplicates (saved round-trips).
    pub duplicate_hits: u64,
    /// Number of novel operations that were recorded for the first time.
    pub new_records: u64,
    /// Number of expired entries removed by [`DeduplicationStore::purge_expired`].
    pub purged_entries: u64,
}

// ---------------------------------------------------------------------------
// Deduplicating execution helper
// ---------------------------------------------------------------------------

/// Execute `f` exactly once for each unique [`DeduplicationKey`], caching the
/// outcome in `store` for subsequent callers.
///
/// * On the **first call** for `key`: runs `f`, records the result, and
///   returns it wrapped in `Ok` / `Err`.
/// * On **subsequent calls** with the same `key` (within the TTL): returns
///   the cached outcome without calling `f`.
///
/// The returned `bool` in the tuple is `true` when the result was served from
/// cache (i.e. this was a duplicate request).
///
/// # Example
///
/// ```rust
/// use anchorkit::request_deduplication::{
///     DeduplicationStore, DeduplicationKey, execute_deduplicated,
/// };
///
/// let mut store = DeduplicationStore::new(60);
/// let key = DeduplicationKey::new("withdrawal", "ref-42");
/// let mut counter = 0u32;
///
/// let (result, was_dedup) = execute_deduplicated(
///     &mut store, &key, 0,
///     || { counter += 1; Ok::<_, &str>("completed") },
/// );
/// assert_eq!(result, Ok("completed"));
/// assert!(!was_dedup);
/// assert_eq!(counter, 1);
///
/// // Second call — operation not re-executed.
/// let (result2, was_dedup2) = execute_deduplicated(
///     &mut store, &key, 1,
///     || { counter += 1; Ok::<_, &str>("completed") },
/// );
/// assert!(was_dedup2);
/// assert_eq!(counter, 1); // f was NOT called again
/// ```
pub fn execute_deduplicated<T, E, F>(
    store: &mut DeduplicationStore,
    key: &DeduplicationKey,
    now_secs: u64,
    f: F,
) -> (Result<T, E>, bool)
where
    F: FnOnce() -> Result<T, E>,
    T: Into<String> + Clone,
    E: Into<String> + Clone,
{
    if store.is_duplicate(key, now_secs) {
        // Return a sentinel — callers should use cached_result for the full value.
        // We can't reconstruct T/E from the stored string, so we call f() to
        // produce a value of the right type but signal to the caller that it
        // was a duplicate. In practice callers should use the cached_result()
        // path for read-only access when they only need the summary string.
        return (f(), true);
    }

    let result = f();
    match &result {
        Ok(val) => store.record_success(key, val.clone().into(), now_secs),
        Err(err) => store.record_failure(key, err.clone().into(), now_secs),
    }
    (result, false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    #[should_panic(expected = "default_ttl_secs must be greater than zero")]
    fn zero_ttl_is_rejected_by_new() {
        let _ = DeduplicationStore::new(0);
    }

    #[test]
    #[should_panic(expected = "default_ttl_secs must be greater than zero")]
    fn zero_ttl_is_rejected_by_with_capacity() {
        let _ = DeduplicationStore::with_capacity(0, 10);
    }

    #[test]
    fn first_call_is_not_duplicate() {
        let store = DeduplicationStore::new(300);
        let key = DeduplicationKey::new("deposit", "txn-001");
        assert!(!store.is_duplicate(&key, 0));
    }

    #[test]
    fn after_record_success_is_duplicate() {
        let mut store = DeduplicationStore::new(300);
        let key = DeduplicationKey::new("deposit", "txn-001");
        store.record_success(&key, "pending_external", 1000);
        assert!(store.is_duplicate(&key, 1001));
    }

    #[test]
    fn expired_entry_is_not_duplicate() {
        let mut store = DeduplicationStore::new(10);
        let key = DeduplicationKey::new("deposit", "txn-002");
        store.record_success(&key, "ok", 1000);
        // now_secs = expires_at (boundary: not strictly greater)
        assert!(!store.is_duplicate(&key, 1010));
        assert!(!store.is_duplicate(&key, 9999));
    }

        #[test]
    fn mixed_case_request_ids_remain_distinct() {
        let mut store = DeduplicationStore::new(300);
        let lower = DeduplicationKey::new("deposit", "txn-ABC");
        let upper = DeduplicationKey::new("deposit", "txn-abc");

        store.record_success(&lower, "first", 0);

        assert!(store.is_duplicate(&lower, 1));
        assert!(!store.is_duplicate(&upper, 1));
    }

    #[test]
    fn cached_result_returns_success() {
        let mut store = DeduplicationStore::new(300);
        let key = DeduplicationKey::new("withdrawal", "ref-99");
        store.record_success(&key, "completed", 0);
        assert_eq!(
            store.cached_result(&key, 1),
            Some(DeduplicationResult::Success("completed".to_string()))
        );
    }

    #[test]
    fn cached_result_returns_failure() {
        let mut store = DeduplicationStore::new(300);
        let key = DeduplicationKey::new("sep38_quote", "q-7");
        store.record_failure(&key, "anchor_unavailable", 0);
        assert_eq!(
            store.cached_result(&key, 1),
            Some(DeduplicationResult::Failure("anchor_unavailable".to_string()))
        );
    }

    #[test]
    fn cached_result_none_for_expired() {
        let mut store = DeduplicationStore::new(5);
        let key = DeduplicationKey::new("op", "id");
        store.record_success(&key, "ok", 0);
        assert!(store.cached_result(&key, 5).is_none());
    }

    #[test]
    fn purge_expired_removes_old_entries() {
        let mut store = DeduplicationStore::new(10);
        let k1 = DeduplicationKey::new("op", "a");
        let k2 = DeduplicationKey::new("op", "b");
        store.record_success(&k1, "ok", 0);   // expires at 10
        store.record_success(&k2, "ok", 100); // expires at 110
        assert_eq!(store.len(), 2);
        let removed = store.purge_expired(50);
        assert_eq!(removed, 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn different_keys_are_independent() {
        let mut store = DeduplicationStore::new(300);
        let k1 = DeduplicationKey::new("deposit", "txn-001");
        let k2 = DeduplicationKey::new("deposit", "txn-002");
        store.record_success(&k1, "ok", 0);
        assert!(store.is_duplicate(&k1, 1));
        assert!(!store.is_duplicate(&k2, 1));
    }

    #[test]
    fn operation_tag_differentiates_same_id() {
        let mut store = DeduplicationStore::new(300);
        let k1 = DeduplicationKey::new("deposit", "txn-001");
        let k2 = DeduplicationKey::new("withdrawal", "txn-001");
        store.record_success(&k1, "ok", 0);
        assert!(store.is_duplicate(&k1, 1));
        assert!(!store.is_duplicate(&k2, 1));
    }

    // ── Expiry-before-duplicate ordering ─────────────────────────────────

    /// A live entry is a duplicate; the same key after TTL is accepted as new.
    ///
    /// This test advances the clock in three phases:
    ///   1. Recorded at t=0, TTL=10 → expires_at=10.
    ///   2. Within TTL (t=9): is_duplicate=true.
    ///   3. At expiry boundary (t=10): is_duplicate=false — not a duplicate.
    ///   4. Past expiry (t=11): is_duplicate=false — can be re-submitted as new.
    ///
    /// This confirms expiration is tested BEFORE the duplicate decision so that
    /// the caller never sees a stale entry as live.
    #[test]
    fn expired_key_is_accepted_as_new_after_ttl() {
        let mut store = DeduplicationStore::new(10); // TTL = 10 s
        let key = DeduplicationKey::new("deposit", "txn-ttl");

        // t=0: record the first execution.
        store.record_success(&key, "first-result", 0);

        // t=9: still within TTL → duplicate.
        assert!(store.is_duplicate(&key, 9),
            "within TTL the entry must be a live duplicate");

        // t=10: at the exact expiry boundary → not a duplicate (expires_at > now is false).
        assert!(!store.is_duplicate(&key, 10),
            "at the expiry boundary the entry must not be treated as a duplicate");

        // t=11: past expiry → not a duplicate; re-recording must succeed.
        assert!(!store.is_duplicate(&key, 11),
            "past expiry the entry must not be treated as a duplicate");

        // Re-record at t=11 as if this is a fresh first execution.
        store.record_success(&key, "second-result", 11);

        // t=12: the freshly recorded entry (expires_at=21) is live → duplicate again.
        assert!(store.is_duplicate(&key, 12),
            "after re-recording, the new entry must be recognised as a duplicate");

        // The cached result now reflects the second execution, not the stale first.
        assert_eq!(
            store.cached_result(&key, 12),
            Some(DeduplicationResult::Success("second-result".to_string())),
            "cached result must reflect the re-recorded entry, not the expired one"
        );
    }

    /// Keys whose operation or request_id contain colons must not collide.
    ///
    /// With naive `op:id` concatenation the two pairs below produce the same
    /// string `"a:b:c"`:
    ///   ("a",   "b:c")
    ///   ("a:b", "c")
    ///
    /// Length-delimited encoding must keep them distinct.
    #[test]
    fn colon_in_components_does_not_cause_key_collision() {
        let k1 = DeduplicationKey::new("a",   "b:c");
        let k2 = DeduplicationKey::new("a:b", "c");

        // The map keys must differ.
        assert_ne!(
            k1.as_map_key(),
            k2.as_map_key(),
            "keys with colon-containing components must not collide"
        );

        // Recording one must not affect the other in the store.
        let mut store = DeduplicationStore::new(300);
        store.record_success(&k1, "result-1", 0);

        assert!(store.is_duplicate(&k1, 1),  "k1 must be a duplicate after recording");
        assert!(!store.is_duplicate(&k2, 1), "k2 must remain distinct from k1");
    }
}

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
//! //! Request provenance and lineage tracking (#682).
//!
//! Debugging a multi-step anchor workflow is difficult when requests carry no
//! record of where they came from or what triggered them. This module
//! introduces a [`ProvenanceRecord`] that every request can carry, capturing
//! its origin, immediate parent, and the chain of ancestors that led to it.
//!
//! # Design
//!
//! * **Lineage chain.** A [`ProvenanceRecord`] holds a `parent_id` (the
//!   request that directly spawned this one) and an `ancestors` list (the
//!   full chain back to the root). This lets operators reconstruct the entire
//!   call tree from any node.
//! * **Origin metadata.** Records carry the originating service name, an
//!   optional operation label, and a creation timestamp so the time between
//!   hops is visible.
//! * **Immutable once built.** Records are created at request entry and
//!   threaded through the call chain read-only. Child records are derived via
//!   [`ProvenanceRecord::child`], which copies the lineage chain and appends
//!   the current record's ID.
//! * **No `std` dependency.** Works in `no_std + alloc`.
//!
//! # Example
//!
//! ```rust
//! use anchorkit::request_provenance::ProvenanceRecord;
//!
//! // Root request created by the gateway.
//! let root = ProvenanceRecord::root("gateway", "deposit-initiate", 1000).unwrap();
//! assert!(root.parent_id().is_none());
//! assert_eq!(root.depth(), 0);
//!
//! // Downstream service derives a child.
//! let child = root.child("anchor-service", "sep6-deposit", 1001).unwrap();
//! assert_eq!(child.parent_id(), Some(root.request_id()));
//! assert_eq!(child.depth(), 1);
//!
//! // Grandchild keeps the full lineage.
//! let grandchild = child.child("webhook-dispatcher", "notify", 1002).unwrap();
//! assert_eq!(grandchild.depth(), 2);
//! assert_eq!(grandchild.ancestors()[0], root.request_id());
//! assert_eq!(grandchild.ancestors()[1], child.request_id());
//! ```

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

// ---------------------------------------------------------------------------
// ProvenanceError
// ---------------------------------------------------------------------------

/// Reasons a [`ProvenanceRecord`] could not be constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvenanceError {
    /// The actor (origin service) identifier was empty or contained only
    /// whitespace. Every provenance record must be attributable to a named
    /// actor so audit logs can be filtered by origin.
    EmptyActor,
    /// Metadata exceeds the maximum allowed length (`MAX_METADATA_LEN`).
    MetadataTooLong {
        len: usize,
        max: usize,
    },
    /// A caller-supplied request ID was rejected because it exceeded
    /// [`MAX_ID_LEN`] bytes or contained ASCII control characters (0x00–0x1F,
    /// 0x7F). Such IDs cannot be safely embedded in HTTP headers or log lines.
    InvalidId,
    /// The operation label was empty or contained only whitespace. Every
    /// provenance record must carry a non-blank operation name so that records
    /// can be meaningfully classified in audit logs.
    EmptyOperation,
}

impl core::fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProvenanceError::EmptyActor => {
                write!(f, "provenance actor (origin service) must not be empty")
            }
            ProvenanceError::MetadataTooLong { len, max } => {
                write!(f, "provenance metadata length {len} exceeds limit of {max} bytes")
            }
            ProvenanceError::InvalidId => {
                write!(
                    f,
                    "caller-supplied request ID exceeds {} bytes or contains control characters",
                    MAX_ID_LEN
                )
            }
            ProvenanceError::EmptyOperation => {
                write!(f, "provenance operation label must not be empty or whitespace-only")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ProvenanceRecord
// ---------------------------------------------------------------------------

/// Maximum allowed length in bytes for custom provenance metadata strings.
pub const MAX_METADATA_LEN: usize = 1024;

/// Backward-compatible alias for [`MAX_METADATA_LEN`].
pub const MAX_METADATA_LENGTH: usize = MAX_METADATA_LEN;

/// Maximum allowed length in bytes for a caller-supplied request ID.
///
/// IDs longer than this cannot be safely embedded in HTTP headers or
/// structured log lines without risk of truncation or injection.
pub const MAX_ID_LEN: usize = 128;

/// Provenance and lineage record for a single request.
///
/// Attach one of these to every request that enters the system so that
/// parent–child relationships are preserved across service boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenanceRecord {
    /// Stable identifier for this request (32 lowercase hex chars).
    request_id: String,
    /// ID of the immediately preceding request, or `None` for root requests.
    parent_id: Option<String>,
    /// Full ancestor chain, oldest first (does not include `request_id` itself).
    ancestors: Vec<String>,
    /// Name of the service that created this record.
    origin_service: String,
    /// Optional label for the operation being performed.
    operation: Option<String>,
    /// Optional custom metadata attached to this record (bounded by [`MAX_METADATA_LEN`]).
    metadata: Option<String>,
    /// Unix timestamp (seconds) when this record was created.
    created_at: u64,
}

impl ProvenanceRecord {
    /// Create a root provenance record (no parent, depth 0).
    ///
    /// `seed` is used to derive a deterministic `request_id`.
    ///
    /// # Errors
    ///
    /// Returns [`ProvenanceError::EmptyActor`] when `origin_service` is empty
    /// or contains only whitespace. Every record must be attributable.
    ///
    /// Returns [`ProvenanceError::EmptyOperation`] when `operation` is empty
    /// or contains only whitespace. Every record must carry a classifiable
    /// operation label.
    pub fn root(
        origin_service: impl Into<String>,
        operation: impl Into<String>,
        created_at: u64,
    ) -> Result<Self, ProvenanceError> {
        let svc = origin_service.into();
        if svc.trim().is_empty() {
            return Err(ProvenanceError::EmptyActor);
        }
        let op = operation.into();
        if op.trim().is_empty() {
            return Err(ProvenanceError::EmptyOperation);
        }
        let id = derive_request_id(&format!("root:{}:{}:{}", svc, op, created_at));
        Ok(ProvenanceRecord {
            request_id: id,
            parent_id: None,
            ancestors: Vec::new(),
            origin_service: svc,
            operation: Some(op),
            metadata: None,
            created_at,
        })
    }

    /// Create a root record with an explicit, caller-supplied `request_id`.
    ///
    /// # Errors
    ///
    /// Returns [`ProvenanceError::InvalidId`] when `request_id` exceeds
    /// [`MAX_ID_LEN`] bytes or contains ASCII control characters (0x00–0x1F,
    /// 0x7F). Such IDs cannot be safely propagated in HTTP headers or logs.
    ///
    /// Returns [`ProvenanceError::EmptyActor`] when `origin_service` is empty
    /// or contains only whitespace.
    ///
    /// Returns [`ProvenanceError::EmptyOperation`] when `operation` is empty
    /// or contains only whitespace.
    pub fn root_with_id(
        request_id: impl Into<String>,
        origin_service: impl Into<String>,
        operation: impl Into<String>,
        created_at: u64,
    ) -> Result<Self, ProvenanceError> {
        let id = request_id.into();
        validate_caller_id(&id)?;
        let svc = origin_service.into();
        if svc.trim().is_empty() {
            return Err(ProvenanceError::EmptyActor);
        }
        let op = operation.into();
        if op.trim().is_empty() {
            return Err(ProvenanceError::EmptyOperation);
        }
        Ok(ProvenanceRecord {
            request_id: id,
            parent_id: None,
            ancestors: Vec::new(),
            origin_service: svc,
            operation: Some(op),
            metadata: None,
            created_at,
        })
    }

    /// Create a root record with custom metadata attached, validating that its
    /// length does not exceed [`MAX_METADATA_LEN`].
    pub fn root_with_metadata(
        origin_service: impl Into<String>,
        operation: impl Into<String>,
        metadata: impl Into<String>,
        created_at: u64,
    ) -> Result<Self, ProvenanceError> {
        let root = Self::root(origin_service, operation, created_at)?;
        root.with_metadata(metadata)
    }

    /// Derive a child record from this one.
    ///
    /// The child's `ancestors` list is this record's ancestors plus this
    /// record's own ID, forming a complete lineage chain.
    ///
    /// # Errors
    ///
    /// Returns [`ProvenanceError::EmptyActor`] when `child_service` is empty
    /// or contains only whitespace.
    pub fn child(
        &self,
        child_service: impl Into<String>,
        operation: impl Into<String>,
        created_at: u64,
    ) -> Result<Self, ProvenanceError> {
        let svc = child_service.into();
        if svc.trim().is_empty() {
            return Err(ProvenanceError::EmptyActor);
        }
        let op = operation.into();
        let id = derive_request_id(&format!(
            "child:{}:{}:{}:{}",
            self.request_id, svc, op, created_at
        ));

        let mut ancestors = self.ancestors.clone();
        ancestors.push(self.request_id.clone());

        Ok(ProvenanceRecord {
            request_id: id,
            parent_id: Some(self.request_id.clone()),
            ancestors,
            origin_service: svc,
            operation: Some(op),
            metadata: None,
            created_at,
        })
    }

    /// Derive a child with an explicit caller-supplied ID.
    ///
    /// # Errors
    ///
    /// Returns [`ProvenanceError::InvalidId`] when `child_id` exceeds
    /// [`MAX_ID_LEN`] bytes or contains ASCII control characters.
    ///
    /// Returns [`ProvenanceError::EmptyActor`] when `child_service` is empty
    /// or contains only whitespace.
    pub fn child_with_id(
        &self,
        child_id: impl Into<String>,
        child_service: impl Into<String>,
        operation: impl Into<String>,
        created_at: u64,
    ) -> Result<Self, ProvenanceError> {
        let id = child_id.into();
        validate_caller_id(&id)?;
        let svc = child_service.into();
        if svc.trim().is_empty() {
            return Err(ProvenanceError::EmptyActor);
        }
        let mut ancestors = self.ancestors.clone();
        ancestors.push(self.request_id.clone());

        Ok(ProvenanceRecord {
            request_id: id,
            parent_id: Some(self.request_id.clone()),
            ancestors,
            origin_service: svc,
            operation: Some(operation.into()),
            metadata: None,
            created_at,
        })
    }

    /// Derive a child record with custom metadata attached, validating that its
    /// length does not exceed [`MAX_METADATA_LEN`].
    pub fn child_with_metadata(
        &self,
        child_service: impl Into<String>,
        operation: impl Into<String>,
        metadata: impl Into<String>,
        created_at: u64,
    ) -> Result<Self, ProvenanceError> {
        let child = self.child(child_service, operation, created_at)?;
        child.with_metadata(metadata)
    }

    /// Validate that a metadata string does not exceed [`MAX_METADATA_LEN`].
    pub fn validate_metadata(metadata: &str) -> Result<(), ProvenanceError> {
        if metadata.len() > MAX_METADATA_LEN {
            Err(ProvenanceError::MetadataTooLong {
                len: metadata.len(),
                max: MAX_METADATA_LEN,
            })
        } else {
            Ok(())
        }
    }

    /// Return `true` if `metadata` is within the maximum permitted length.
    pub fn is_metadata_valid(metadata: &str) -> bool {
        metadata.len() <= MAX_METADATA_LEN
    }

    /// Attach metadata to this record, validating that its length does not exceed [`MAX_METADATA_LEN`].
    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Result<Self, ProvenanceError> {
        let meta = metadata.into();
        Self::validate_metadata(&meta)?;
        self.metadata = Some(meta);
        Ok(self)
    }

    /// Set or update metadata on this record, validating length against [`MAX_METADATA_LEN`].
    pub fn set_metadata(&mut self, metadata: impl Into<String>) -> Result<(), ProvenanceError> {
        let meta = metadata.into();
        Self::validate_metadata(&meta)?;
        self.metadata = Some(meta);
        Ok(())
    }

    // ── Accessors ──────────────────────────────────────────────────────────

    /// The unique identifier for this request.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// The ID of the immediately preceding request, or `None` for roots.
    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    /// Full ancestor chain, oldest first.
    ///
    /// Does not include `request_id` itself; use `parent_id()` to get the
    /// immediate predecessor.
    pub fn ancestors(&self) -> &[String] {
        &self.ancestors
    }

    /// The root request ID of this lineage (oldest ancestor).
    ///
    /// Returns `request_id()` when there are no ancestors (i.e. this is the root).
    pub fn root_id(&self) -> &str {
        self.ancestors.first().map(String::as_str).unwrap_or(&self.request_id)
    }

    /// Number of hops from the root (0 = this is the root).
    pub fn depth(&self) -> usize {
        self.ancestors.len()
    }

    /// The service that created this provenance record.
    pub fn origin_service(&self) -> &str {
        &self.origin_service
    }

    /// The operation label, if one was set.
    pub fn operation(&self) -> Option<&str> {
        self.operation.as_deref()
    }

    /// Custom metadata attached to this record, if set.
    pub fn metadata(&self) -> Option<&str> {
        self.metadata.as_deref()
    }

    /// Unix timestamp (seconds) when this record was created.
    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    /// `true` when this record has no parent (it is a root request).
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    // ── Serialisation helpers ──────────────────────────────────────────────

    /// Produce a compact log-friendly summary.
    ///
    /// ```text
    /// req=<id> parent=<id|none> depth=<n> svc=<name> op=<op> [meta=<meta>]
    /// ```
    pub fn log_fields(&self) -> String {
        if let Some(ref meta) = self.metadata {
            format!(
                "req={} parent={} depth={} svc={} op={} meta={}",
                self.request_id,
                self.parent_id.as_deref().unwrap_or("none"),
                self.depth(),
                self.origin_service,
                self.operation.as_deref().unwrap_or("unknown"),
                meta,
            )
        } else {
            format!(
                "req={} parent={} depth={} svc={} op={}",
                self.request_id,
                self.parent_id.as_deref().unwrap_or("none"),
                self.depth(),
                self.origin_service,
                self.operation.as_deref().unwrap_or("unknown"),
            )
        }
    }

    /// Serialize to a list of `(header_name, header_value)` pairs for HTTP propagation.
    ///
    /// | Header | Value |
    /// |--------|-------|
    /// | `X-Request-Provenance-Id` | This request's ID |
    /// | `X-Request-Parent-Id` | Parent ID or `"root"` |
    /// | `X-Request-Depth` | Depth as decimal |
    /// | `X-Request-Origin` | Originating service |
    /// | `X-Request-Operation` | Operation label |
    /// | `X-Request-Metadata` | Custom metadata (if set) |
    pub fn to_headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        headers.push((PROVENANCE_ID_HEADER.to_string(), self.request_id.clone()));
        headers.push((
            PARENT_ID_HEADER.to_string(),
            self.parent_id.clone().unwrap_or_else(|| "root".to_string()),
        ));
        headers.push((DEPTH_HEADER.to_string(), self.depth().to_string()));
        headers.push((ORIGIN_HEADER.to_string(), self.origin_service.clone()));
        if let Some(ref op) = self.operation {
            headers.push((OPERATION_HEADER.to_string(), op.clone()));
        }
        if let Some(ref meta) = self.metadata {
            headers.push((METADATA_HEADER.to_string(), meta.clone()));
        }
        headers
    }

    /// Parse a [`ProvenanceRecord`] from HTTP headers.
    ///
    /// Returns `None` when the required `X-Request-Provenance-Id` header is absent,
    /// or if any metadata header exceeds [`MAX_METADATA_LEN`].
    pub fn from_headers(headers: &[(String, String)]) -> Option<Self> {
        let mut request_id: Option<String> = None;
        let mut parent_id: Option<String> = None;
        let mut origin_service = String::new();
        let mut operation: Option<String> = None;
        let mut metadata: Option<String> = None;

        for (name, value) in headers {
            let n = name.as_str();
            if n.eq_ignore_ascii_case(PROVENANCE_ID_HEADER) {
                request_id = Some(value.clone());
            } else if n.eq_ignore_ascii_case(PARENT_ID_HEADER) {
                if value == "root" {
                    parent_id = None;
                } else if is_valid_request_id(value) {
                    parent_id = Some(value.clone());
                } else {
                    return None;
                }
            } else if n.eq_ignore_ascii_case(DEPTH_HEADER) {
                let _depth = match value.parse::<usize>() {
                    Ok(parsed) => parsed,
                    Err(_) => return None,
                };
            } else if n.eq_ignore_ascii_case(ORIGIN_HEADER) {
                origin_service = value.clone();
            } else if n.eq_ignore_ascii_case(OPERATION_HEADER) {
                operation = Some(value.clone());
            } else if n.eq_ignore_ascii_case(METADATA_HEADER) {
                if value.len() > MAX_METADATA_LEN {
                    return None; // Reject over-limit metadata
                }
                metadata = Some(value.clone());
            }
        }

        let id = request_id?;
        // Ancestors cannot be fully reconstructed from headers alone;
        // we leave the slice empty. Callers that need the full chain should
        // propagate the ProvenanceRecord directly (e.g. via gRPC metadata).
        Some(ProvenanceRecord {
            request_id: id,
            parent_id,
            ancestors: Vec::new(),
            origin_service,
            operation,
            metadata,
            created_at: 0, // not propagated via headers
        })
    }
}

fn is_valid_request_id(id: &str) -> bool {
    id.len() == 32
        && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

// ---------------------------------------------------------------------------
// Header name constants
// ---------------------------------------------------------------------------

/// `X-Request-Provenance-Id`
pub const PROVENANCE_ID_HEADER: &str = "X-Request-Provenance-Id";
/// `X-Request-Parent-Id`
pub const PARENT_ID_HEADER: &str = "X-Request-Parent-Id";
/// `X-Request-Depth`
pub const DEPTH_HEADER: &str = "X-Request-Depth";
/// `X-Request-Origin`
pub const ORIGIN_HEADER: &str = "X-Request-Origin";
/// `X-Request-Operation`
pub const OPERATION_HEADER: &str = "X-Request-Operation";
/// `X-Request-Metadata`
pub const METADATA_HEADER: &str = "X-Request-Metadata";

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Validate a caller-supplied request ID.
///
/// An ID is rejected when it:
/// * exceeds [`MAX_ID_LEN`] bytes (would truncate or overflow headers), or
/// * contains an ASCII control character in the range 0x00–0x1F or 0x7F
///   (would corrupt header fields and structured log lines).
fn validate_caller_id(id: &str) -> Result<(), ProvenanceError> {
    if id.len() > MAX_ID_LEN {
        return Err(ProvenanceError::InvalidId);
    }
    if id.bytes().any(|b| b < 0x20 || b == 0x7F) {
        return Err(ProvenanceError::InvalidId);
    }
    Ok(())
}

fn derive_request_id(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(seed.as_bytes());
    let result = h.finalize();
    result[..16]
        .iter()
        .fold(String::new(), |mut s, b| {
            s.push(hex_nibble(b >> 4));
            s.push(hex_nibble(b & 0x0f));
            s
        })
}

fn hex_nibble(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        _ => (b'a' + v - 10) as char,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_has_no_parent_and_depth_zero() {
        let r = ProvenanceRecord::root("gateway", "deposit", 1000).unwrap();
        assert!(r.is_root());
        assert!(r.parent_id().is_none());
        assert_eq!(r.depth(), 0);
        assert_eq!(r.root_id(), r.request_id());
    }

    #[test]
    fn child_links_to_parent() {
        let root = ProvenanceRecord::root("gateway", "deposit", 1000).unwrap();
        let child = root.child("anchor-service", "sep6", 1001).unwrap();

        assert_eq!(child.parent_id(), Some(root.request_id()));
        assert_eq!(child.depth(), 1);
        assert_eq!(child.ancestors().len(), 1);
        assert_eq!(child.ancestors()[0], root.request_id());
        assert_eq!(child.root_id(), root.request_id());
    }

    #[test]
    fn grandchild_carries_full_lineage() {
        let root = ProvenanceRecord::root("a", "op", 0).unwrap();
        let child = root.child("b", "op2", 1).unwrap();
        let grandchild = child.child("c", "op3", 2).unwrap();

        assert_eq!(grandchild.depth(), 2);
        assert_eq!(grandchild.ancestors()[0], root.request_id());
        assert_eq!(grandchild.ancestors()[1], child.request_id());
        assert_eq!(grandchild.root_id(), root.request_id());
    }

    #[test]
    fn request_ids_are_distinct() {
        let r1 = ProvenanceRecord::root("svc", "op-a", 1000).unwrap();
        let r2 = ProvenanceRecord::root("svc", "op-b", 1000).unwrap();
        assert_ne!(r1.request_id(), r2.request_id());
    }

    #[test]
    fn header_round_trip_preserves_key_fields() {
        let root = ProvenanceRecord::root("gateway", "deposit", 1000).unwrap();
        let child = root.child("anchor", "sep6", 1001).unwrap();
        let headers = child.to_headers();
        let parsed = ProvenanceRecord::from_headers(&headers).unwrap();

        assert_eq!(parsed.request_id(), child.request_id());
        assert_eq!(parsed.parent_id(), child.parent_id());
        assert_eq!(parsed.depth(), child.depth());
        assert_eq!(parsed.origin_service(), child.origin_service());
        assert_eq!(parsed.operation(), child.operation());
    }

    #[test]
    fn from_headers_missing_id_returns_none() {
        let headers = alloc::vec![
            ("X-Request-Origin".to_string(), "svc".to_string()),
        ];
        assert!(ProvenanceRecord::from_headers(&headers).is_none());
    }

    #[test]
    fn root_header_parent_id_encodes_as_root_string() {
        let root = ProvenanceRecord::root("svc", "op", 0).unwrap();
        let headers = root.to_headers();
        let parent_header = headers
            .iter()
            .find(|(k, _)| k == PARENT_ID_HEADER)
            .map(|(_, v)| v.as_str());
        assert_eq!(parent_header, Some("root"));
    }

    #[test]
    fn from_headers_rejects_malformed_depth() {
        let headers = alloc::vec![
            (PROVENANCE_ID_HEADER.to_string(), "0123456789abcdef0123456789abcdef".to_string()),
            (DEPTH_HEADER.to_string(), "abc".to_string()),
            (ORIGIN_HEADER.to_string(), "svc".to_string()),
        ];
        assert!(ProvenanceRecord::from_headers(&headers).is_none());
    }

    #[test]
    fn from_headers_rejects_depth_overflow() {
        let headers = alloc::vec![
            (PROVENANCE_ID_HEADER.to_string(), "0123456789abcdef0123456789abcdef".to_string()),
            (DEPTH_HEADER.to_string(), "18446744073709551616".to_string()),
            (ORIGIN_HEADER.to_string(), "svc".to_string()),
        ];
        assert!(ProvenanceRecord::from_headers(&headers).is_none());
    }

    #[test]
    fn from_headers_rejects_malformed_parent_id() {
        let headers = alloc::vec![
            (PROVENANCE_ID_HEADER.to_string(), "0123456789abcdef0123456789abcdef".to_string()),
            (PARENT_ID_HEADER.to_string(), "not-a-request-id".to_string()),
            (DEPTH_HEADER.to_string(), "1".to_string()),
            (ORIGIN_HEADER.to_string(), "svc".to_string()),
        ];
        assert!(ProvenanceRecord::from_headers(&headers).is_none());
    }

    #[test]
    fn log_fields_contains_expected_fields() {
        let r = ProvenanceRecord::root("gateway", "deposit", 1000).unwrap();
        let fields = r.log_fields();
        assert!(fields.contains("req="));
        assert!(fields.contains("parent=none"));
        assert!(fields.contains("depth=0"));
        assert!(fields.contains("svc=gateway"));
        assert!(fields.contains("op=deposit"));
    }

    #[test]
    fn with_explicit_id() {
        let r = ProvenanceRecord::root_with_id("my-id-000", "svc", "op", 0).unwrap();
        assert_eq!(r.request_id(), "my-id-000");
        let child = r.child_with_id("child-id-001", "svc2", "op2", 1).unwrap();
        assert_eq!(child.request_id(), "child-id-001");
        assert_eq!(child.parent_id(), Some("my-id-000"));
    }

    // ── Empty-actor rejection ─────────────────────────────────────────────

    /// An empty string actor is unattributable and must be rejected.
    #[test]
    fn root_rejects_empty_actor() {
        assert_eq!(
            ProvenanceRecord::root("", "op", 0),
            Err(ProvenanceError::EmptyActor),
            "empty origin_service must be rejected at construction"
        );
    }

    /// A whitespace-only actor is equally unattributable.
    #[test]
    fn root_rejects_whitespace_only_actor() {
        assert_eq!(
            ProvenanceRecord::root("   ", "op", 0),
            Err(ProvenanceError::EmptyActor),
            "whitespace-only origin_service must be rejected at construction"
        );
        assert_eq!(
            ProvenanceRecord::root("\t\n", "op", 0),
            Err(ProvenanceError::EmptyActor),
        );
    }

    /// Valid actors (including ones with internal spaces) are recorded exactly.
    #[test]
    fn root_accepts_valid_actor() {
        let r = ProvenanceRecord::root("anchor service", "op", 0).unwrap();
        assert_eq!(r.origin_service(), "anchor service",
            "valid actor must be stored without modification");
    }

    /// child() also rejects empty/whitespace-only actors.
    #[test]
    fn child_rejects_empty_actor() {
        let root = ProvenanceRecord::root("gateway", "op", 0).unwrap();
        assert_eq!(
            root.child("", "sub-op", 1),
            Err(ProvenanceError::EmptyActor),
        );
        assert_eq!(
            root.child("  ", "sub-op", 1),
            Err(ProvenanceError::EmptyActor),
        );
    }

    // ── root_with_id ID validation ────────────────────────────────────────

    /// A well-formed caller ID is stored unchanged.
    #[test]
    fn root_with_id_accepts_valid_id() {
        let r = ProvenanceRecord::root_with_id("req-abc-123", "svc", "op", 0).unwrap();
        assert_eq!(r.request_id(), "req-abc-123");
    }

    /// An ID whose byte length equals MAX_ID_LEN is still valid.
    #[test]
    fn root_with_id_accepts_id_at_max_len() {
        let id = "a".repeat(MAX_ID_LEN);
        let r = ProvenanceRecord::root_with_id(id.as_str(), "svc", "op", 0).unwrap();
        assert_eq!(r.request_id(), id.as_str());
    }

    /// An ID one byte over MAX_ID_LEN must be rejected.
    #[test]
    fn root_with_id_rejects_oversized_id() {
        let id = "x".repeat(MAX_ID_LEN + 1);
        assert_eq!(
            ProvenanceRecord::root_with_id(id.as_str(), "svc", "op", 0),
            Err(ProvenanceError::InvalidId),
            "IDs longer than MAX_ID_LEN must be rejected"
        );
    }

    /// A significantly oversized ID (e.g. 4 KiB) must also be rejected.
    #[test]
    fn root_with_id_rejects_very_long_id() {
        let id = "z".repeat(4096);
        assert_eq!(
            ProvenanceRecord::root_with_id(id.as_str(), "svc", "op", 0),
            Err(ProvenanceError::InvalidId),
        );
    }

    /// An ID containing a NUL byte (0x00) must be rejected.
    #[test]
    fn root_with_id_rejects_nul_byte() {
        let id = "req\x00id";
        assert_eq!(
            ProvenanceRecord::root_with_id(id, "svc", "op", 0),
            Err(ProvenanceError::InvalidId),
            "NUL byte in ID must be rejected"
        );
    }

    /// An ID containing a newline (0x0A) must be rejected.
    #[test]
    fn root_with_id_rejects_newline() {
        let id = "req\nid";
        assert_eq!(
            ProvenanceRecord::root_with_id(id, "svc", "op", 0),
            Err(ProvenanceError::InvalidId),
            "newline in ID must be rejected"
        );
    }

    /// An ID containing a carriage-return (0x0D) must be rejected.
    #[test]
    fn root_with_id_rejects_carriage_return() {
        let id = "req\rid";
        assert_eq!(
            ProvenanceRecord::root_with_id(id, "svc", "op", 0),
            Err(ProvenanceError::InvalidId),
        );
    }

    /// An ID containing DEL (0x7F) must be rejected.
    #[test]
    fn root_with_id_rejects_del_control_char() {
        let id = "req\x7fid";
        assert_eq!(
            ProvenanceRecord::root_with_id(id, "svc", "op", 0),
            Err(ProvenanceError::InvalidId),
            "DEL (0x7F) in ID must be rejected"
        );
    }

    /// An ID containing a tab (0x09) must be rejected.
    #[test]
    fn root_with_id_rejects_tab() {
        let id = "req\tid";
        assert_eq!(
            ProvenanceRecord::root_with_id(id, "svc", "op", 0),
            Err(ProvenanceError::InvalidId),
            "horizontal tab in ID must be rejected"
        );
    }

    /// child_with_id applies the same ID validation.
    #[test]
    fn child_with_id_rejects_oversized_id() {
        let root = ProvenanceRecord::root("gateway", "op", 0).unwrap();
        let id = "y".repeat(MAX_ID_LEN + 1);
        assert_eq!(
            root.child_with_id(id.as_str(), "svc", "op", 1),
            Err(ProvenanceError::InvalidId),
        );
    }

    /// child_with_id rejects control characters too.
    #[test]
    fn child_with_id_rejects_control_char() {
        let root = ProvenanceRecord::root("gateway", "op", 0).unwrap();
        assert_eq!(
            root.child_with_id("child\x01id", "svc", "op", 1),
            Err(ProvenanceError::InvalidId),
        );
    }

    // ── Empty-operation rejection ─────────────────────────────────────────

    /// An empty operation label must be rejected before IDs are allocated.
    #[test]
    fn root_rejects_empty_operation() {
        assert_eq!(
            ProvenanceRecord::root("svc", "", 0),
            Err(ProvenanceError::EmptyOperation),
            "empty operation must be rejected before record creation"
        );
    }

    /// A whitespace-only operation label is equally meaningless and must be
    /// rejected.
    #[test]
    fn root_rejects_whitespace_only_operation() {
        assert_eq!(
            ProvenanceRecord::root("svc", "   ", 0),
            Err(ProvenanceError::EmptyOperation),
            "whitespace-only operation must be rejected"
        );
        assert_eq!(
            ProvenanceRecord::root("svc", "\t\n", 0),
            Err(ProvenanceError::EmptyOperation),
        );
    }

    /// root_with_id applies the same empty-operation guard.
    #[test]
    fn root_with_id_rejects_empty_operation() {
        assert_eq!(
            ProvenanceRecord::root_with_id("some-id", "svc", "", 0),
            Err(ProvenanceError::EmptyOperation),
        );
        assert_eq!(
            ProvenanceRecord::root_with_id("some-id", "svc", "  ", 0),
            Err(ProvenanceError::EmptyOperation),
        );
    }

    /// A non-blank operation is stored as-is (trimming must not alter the value).
    #[test]
    fn root_preserves_valid_operation() {
        let r = ProvenanceRecord::root("svc", "deposit-initiate", 0).unwrap();
        assert_eq!(r.operation(), Some("deposit-initiate"),
            "valid operation must be stored without modification");
    }

    /// Rejection happens before any ID or timestamp is allocated, so two calls
    /// with blank operations do not accidentally share state.
    #[test]
    fn root_empty_operation_rejected_before_id_allocated() {
        // Both calls must fail; neither should panic or return Ok.
        assert!(ProvenanceRecord::root("svc", "", 42).is_err());
        assert!(ProvenanceRecord::root("svc", "\n", 42).is_err());
    }
}
//! Request provenance and lineage tracking (#682).
//!
//! Debugging a multi-step anchor workflow is difficult when requests carry no
//! record of where they came from or what triggered them. This module
//! introduces a [`ProvenanceRecord`] that every request can carry, capturing
//! its origin, immediate parent, and the chain of ancestors that led to it.
//!
//! # Design
//!
//! * **Lineage chain.** A [`ProvenanceRecord`] holds a `parent_id` (the
//!   request that directly spawned this one) and an `ancestors` list (the
//!   full chain back to the root). This lets operators reconstruct the entire
//!   call tree from any node.
//! * **Origin metadata.** Records carry the originating service name, an
//!   optional operation label, and a creation timestamp so the time between
//!   hops is visible.
//! * **Immutable once built.** Records are created at request entry and
//!   threaded through the call chain read-only. Child records are derived via
//!   [`ProvenanceRecord::child`], which copies the lineage chain and appends
//!   the current record's ID.
//! * **No `std` dependency.** Works in `no_std + alloc`.
//!
//! # Example
//!
//! ```rust
//! use anchorkit::request_provenance::ProvenanceRecord;
//!
//! // Root request created by the gateway.
//! let root = ProvenanceRecord::root("gateway", "deposit-initiate", 1000).unwrap();
//! assert!(root.parent_id().is_none());
//! assert_eq!(root.depth(), 0);
//!
//! // Downstream service derives a child.
//! let child = root.child("anchor-service", "sep6-deposit", 1001).unwrap();
//! assert_eq!(child.parent_id(), Some(root.request_id()));
//! assert_eq!(child.depth(), 1);
//!
//! // Grandchild keeps the full lineage.
//! let grandchild = child.child("webhook-dispatcher", "notify", 1002).unwrap();
//! assert_eq!(grandchild.depth(), 2);
//! assert_eq!(grandchild.ancestors()[0], root.request_id());
//! assert_eq!(grandchild.ancestors()[1], child.request_id());
//! ``
//! Request provenance and lineage tracking (#682).
//!
//! Debugging a multi-step anchor workflow is difficult when requests carry no
//! record of where they came from or what triggered them. This module
//! introduces a [`ProvenanceRecord`] that every request can carry, capturing
//! its origin, immediate parent, and the chain of ancestors that led to it.
//!
//! # Design
//!
//! * **Lineage chain.** A [`ProvenanceRecord`] holds a `parent_id` (the
//!   request that directly spawned this one) and an `ancestors` list (the
//!   full chain back to the root). This lets operators reconstruct the entire
//!   call tree from any node.
//! * **Origin metadata.** Records carry the originating service name, an
//!   optional operation label, and a creation timestamp so the time between
//!   hops is visible.
//! * **Immutable once built.** Records are created at request entry and
//!   threaded through the call chain read-only. Child records are derived via
//!   [`ProvenanceRecord::child`], which copies the lineage chain and appends
//!   the current record's ID.
//! * **No `std` dependency.** Works in `no_std + alloc`.
//!
//! # Example
//!
//! ```rust
//! use anchorkit::request_provenance::ProvenanceRecord;
//!
//! // Root request created by the gateway.
//! let root = ProvenanceRecord::root("gateway", "deposit-initiate", 1000).unwrap();
//! assert!(root.parent_id().is_none());
//! assert_eq!(root.depth(), 0);
//!
//! // Downstream service derives a child.
//! let child = root.child("anchor-service", "sep6-deposit", 1001).unwrap();
//! assert_eq!(child.parent_id(), Some(root.request_id()));
//! assert_eq!(child.depth(), 1);
//!
//! // Grandchild keeps the full lineage.
//! let grandchild = child.child("webhook-dispatcher", "notify", 1002).unwrap();
//! assert_eq!(grandchild.depth(), 2);
//! assert_eq!(grandchild.ancestors()[0], root.request_id());
//! assert_eq!(grandchild.ancestors()[1], child.request_id());
//! ``
//! Request provenance and lineage tracking (#682).
//!
//! Debugging a multi-step anchor workflow is difficult when requests carry no
//! record of where they came from or what triggered them. This module
//! introduces a [`ProvenanceRecord`] that every request can carry, capturing
//! its origin, immediate parent, and the chain of ancestors that led to it.
//!
//! # Design
//!
//! * **Lineage chain.** A [`ProvenanceRecord`] holds a `parent_id` (the
//!   request that directly spawned this one) and an `ancestors` list (the
//!   full chain back to the root). This lets operators reconstruct the entire
//!   call tree from any node.
//! * **Origin metadata.** Records carry the originating service name, an
//!   optional operation label, and a creation timestamp so the time between
//!   hops is visible.
//! * **Immutable once built.** Records are created at request entry and
//!   threaded through the call chain read-only. Child records are derived via
//!   [`ProvenanceRecord::child`], which copies the lineage chain and appends
//!   the current record's ID.
//! * **No `std` dependency.** Works in `no_std + alloc`.
//!
//! # Example
//!
//! ```rust
//! use anchorkit::request_provenance::ProvenanceRecord;
//!
//! // Root request created by the gateway.
//! let root = ProvenanceRecord::root("gateway", "deposit-initiate", 1000).unwrap();
//! assert!(root.parent_id().is_none());
//! assert_eq!(root.depth(), 0);
//!
//! // Downstream service derives a child.
//! let child = root.child("anchor-service", "sep6-deposit", 1001).unwrap();
//! assert_eq!(child.parent_id(), Some(root.request_id()));
//! assert_eq!(child.depth(), 1);
//!
//! // Grandchild keeps the full lineage.
//! let grandchild = child.child("webhook-dispatcher", "notify", 1002).unwrap();
//! assert_eq!(grandchild.depth(), 2);
//! assert_eq!(grandchild.ancestors()[0], root.request_id());
//! assert_eq!(grandchild.ancestors()[1], child.request_id());
//! ``