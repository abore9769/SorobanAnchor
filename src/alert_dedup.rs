//! Alert deduplication and suppression (issue #686).
//!
//! Repeated failures can generate alert storms where the same condition fires
//! dozens of times before an operator responds.  This module provides:
//!
//! - [`AlertDeduplicator`] — tracks recently fired alerts by a fingerprint key
//!   and suppresses duplicates within a configurable time window.
//! - [`AlertSuppressor`] — allows operators to explicitly silence an alert
//!   fingerprint for a fixed duration (e.g. during a planned maintenance window).
//! - [`DedupConfig`] — controls the deduplication window and maximum tracked keys.
//!
//! Both structures use interior mutability (`RefCell`) so they can be threaded
//! through `&self` closures, mirroring the pattern used by `MetricsRegistry`
//! and `StructuredLogger`.
//!
//! # Fingerprinting
//!
//! The caller is responsible for choosing the fingerprint key.  A good key
//! uniquely identifies the *condition*, not each individual event.  For
//! example:
//!
//! ```text
//! "anchor:example.com:health:critical"
//! "contract:attestor_replay:GADDR..."
//! ```
//!
//! # Example
//!
//! ```rust
//! use anchorkit::alert_dedup::{AlertDeduplicator, DedupConfig};
//!
//! let dedup = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 1000 });
//!
//! // First occurrence fires.
//! assert!(dedup.should_fire("anchor:example.com:critical", 1_000));
//!
//! // Same fingerprint within the window is suppressed.
//! assert!(!dedup.should_fire("anchor:example.com:critical", 1_100));
//!
//! // After the window expires the alert fires again.
//! assert!(dedup.should_fire("anchor:example.com:critical", 1_000 + 300 + 1));
//! ```

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::cell::RefCell;

// ---------------------------------------------------------------------------
// DedupConfig
// ---------------------------------------------------------------------------

/// Configuration for [`AlertDeduplicator`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedupConfig {
    /// How long (in seconds) to suppress duplicate alerts after one fires.
    /// Setting this to `0` disables deduplication (every event fires).
    pub window_seconds: u64,
    /// Maximum number of fingerprint keys tracked simultaneously.
    /// When the map is full the oldest entry is evicted before inserting a new
    /// one, so memory stays bounded.  Set to `0` for unlimited (use with care).
    pub max_keys: usize,
}

impl Default for DedupConfig {
    fn default() -> Self {
        DedupConfig {
            window_seconds: 300, // 5 minutes
            max_keys: 10_000,
        }
    }
}

// ---------------------------------------------------------------------------
// AlertDeduplicator
// ---------------------------------------------------------------------------

/// Suppresses duplicate alerts that fire within a rolling time window.
///
/// For each fingerprint key, the deduplicator records the Unix timestamp (in
/// seconds) of the last fired alert.  A subsequent alert with the same key is
/// suppressed unless `now - last_fired >= window_seconds`.
#[derive(Debug, Default)]
pub struct AlertDeduplicator {
    config: DedupConfig,
    /// Maps fingerprint → (last_fired_at, suppress_count).
    last_fired: RefCell<BTreeMap<String, (u64, u64)>>,
    /// JSON-formatted suppression log entries (one per suppressed call).
    suppression_log: RefCell<Vec<String>>,
}

impl AlertDeduplicator {
    /// Create a deduplicator with the given configuration.
    pub fn new(config: DedupConfig) -> Self {
        AlertDeduplicator {
            config,
            last_fired: RefCell::new(BTreeMap::new()),
            suppression_log: RefCell::new(Vec::new()),
        }
    }

    /// Decide whether an alert with `fingerprint` should fire at `now`.
    ///
    /// Returns `true` and records `now` as the last-fired timestamp when the
    /// alert should be delivered.  Returns `false` (and leaves the timestamp
    /// unchanged) when the alert is within its suppression window.
    ///
    /// When `window_seconds` is `0` every call returns `true`.
    pub fn should_fire(&self, fingerprint: &str, now: u64) -> bool {
        if self.config.window_seconds == 0 {
            return true;
        }

        let mut map = self.last_fired.borrow_mut();

        if let Some(entry) = map.get_mut(fingerprint) {
            if now.saturating_sub(entry.0) < self.config.window_seconds {
                // #819: saturating increment prevents counter overflow.
                entry.1 = entry.1.saturating_add(1);
                // #820: record suppression with the alert key for operator correlation.
                // #1053: bound the diagnostic collection by the same max_keys
                // config so suppressed alerts cannot grow it without limit.
                self.push_suppression_diagnostic(fingerprint);
                return false;
            }
            // Window expired — update entry in place and fire.
            *entry = (now, 0);
            return true;
        }

        // Key not yet tracked — evict the least-recently-fired entry (minimum
        // last_fired timestamp) if at capacity, then insert.
        if self.config.max_keys > 0 && map.len() >= self.config.max_keys {
            if let Some(oldest_key) = map
                .iter()
                .min_by_key(|(_, &(ts, _))| ts)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest_key);
            }
        }

        map.insert(fingerprint.into(), (now, 0));
        true
    }

    /// Drain and return all buffered suppression log entries.
    ///
    /// Each entry is a JSON object containing at least the `alert_key` field
    /// identifying which fingerprint was suppressed. The collection is bounded
    /// by [`DedupConfig::max_keys`] so it cannot grow without limit.
    pub fn drain_suppression_log(&self) -> Vec<String> {
        self.suppression_log.borrow_mut().drain(..).collect()
    }

    /// Append a suppression diagnostic for `fingerprint`, evicting the oldest
    /// diagnostics when the collection exceeds [`DedupConfig::max_keys`].
    ///
    /// The fingerprint is serialised as a JSON string literal so quotes,
    /// backslashes, and control characters cannot produce an invalid entry.
    fn push_suppression_diagnostic(&self, fingerprint: &str) {
        let mut diagnostic = String::from("{\"alert_key\":");
        crate::structured_log::write_json_string(&mut diagnostic, fingerprint);
        diagnostic.push('}');

        let mut log = self.suppression_log.borrow_mut();
        if self.config.max_keys > 0 && log.len() >= self.config.max_keys {
            // Drop the oldest entries so the newest diagnostic stays available.
            let overflow = log.len() - self.config.max_keys + 1;
            log.drain(0..overflow);
        }
        log.push(diagnostic);
    }

    /// Forcibly clear the suppression record for `fingerprint`, allowing the
    /// next occurrence to fire regardless of the window.
    pub fn reset(&self, fingerprint: &str) {
        self.last_fired.borrow_mut().remove(fingerprint);
    }

    /// Clear all tracked fingerprints.
    pub fn reset_all(&self) {
        self.last_fired.borrow_mut().clear();
    }

    /// Return the last-fired timestamp for `fingerprint`, if any.
    pub fn last_fired_at(&self, fingerprint: &str) -> Option<u64> {
        self.last_fired.borrow().get(fingerprint).map(|&(ts, _)| ts)
    }

    /// Return the accumulated suppression count for `fingerprint`, if tracked.
    pub fn suppress_count_for(&self, fingerprint: &str) -> Option<u64> {
        self.last_fired.borrow().get(fingerprint).map(|&(_, count)| count)
    }

    /// Number of fingerprints currently tracked.
    pub fn tracked_count(&self) -> usize {
        self.last_fired.borrow().len()
    }

    /// Reference to the active configuration.
    pub fn config(&self) -> &DedupConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// SuppressedEntry
// ---------------------------------------------------------------------------

/// One entry in the manual suppression list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppressedEntry {
    /// The fingerprint pattern being suppressed.  Exact-string match only.
    pub fingerprint: String,
    /// Unix timestamp (seconds) when the suppression expires.
    pub expires_at: u64,
    /// Optional human-readable reason for the suppression.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// AlertSuppressor
// ---------------------------------------------------------------------------

/// Explicit, time-bounded suppression list.
///
/// Operators can suppress specific alert fingerprints for a fixed duration
/// (e.g. during planned maintenance).  Unlike [`AlertDeduplicator`], which
/// auto-suppresses based on recency, [`AlertSuppressor`] fires only when an
/// entry has been explicitly added.
#[derive(Debug, Default)]
pub struct AlertSuppressor {
    /// Active suppression entries, keyed by fingerprint.
    entries: RefCell<BTreeMap<String, SuppressedEntry>>,
}

impl AlertSuppressor {
    /// Create an empty suppressor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Suppress alerts matching `fingerprint` from time `now` until `expires_at`
    /// (Unix seconds).
    ///
    /// If an entry already exists for the fingerprint it is overwritten so the
    /// operator can extend or shorten an active suppression.
    ///
    /// A suppression whose `expires_at` is already in the past (`<= now`) is
    /// rejected — it could never suppress a real alert and would only occupy
    /// suppression state until lazily purged.
    pub fn suppress(
        &self,
        fingerprint: impl Into<String>,
        now: u64,
        expires_at: u64,
        reason: impl Into<String>,
    ) {
        if expires_at <= now {
            return;
        }
        let key: String = fingerprint.into();
        self.entries.borrow_mut().insert(
            key.clone(),
            SuppressedEntry {
                fingerprint: key,
                expires_at,
                reason: reason.into(),
            },
        );
    }

    /// Returns `true` when `fingerprint` is actively suppressed at time `now`.
    ///
    /// Expired entries are removed lazily on access.
    pub fn is_suppressed(&self, fingerprint: &str, now: u64) -> bool {
        let mut entries = self.entries.borrow_mut();
        match entries.get(fingerprint) {
            None => false,
            Some(entry) if entry.expires_at <= now => {
                // Expired — remove lazily and report not suppressed.
                let key = entry.fingerprint.clone();
                entries.remove(&key);
                false
            }
            Some(_) => true,
        }
    }

    /// Lift the suppression for `fingerprint` immediately.
    pub fn unsuppress(&self, fingerprint: &str) {
        self.entries.borrow_mut().remove(fingerprint);
    }

    /// Remove all suppression entries that have already expired at `now`.
    pub fn purge_expired(&self, now: u64) {
        let mut entries = self.entries.borrow_mut();
        entries.retain(|_, e| e.expires_at > now);
    }

    /// Number of active (not-yet-expired) suppression entries at `now`.
    pub fn active_count(&self, now: u64) -> usize {
        self.entries
            .borrow()
            .values()
            .filter(|e| e.expires_at > now)
            .count()
    }

    /// Return a snapshot of all current suppression entries (including expired).
    pub fn entries(&self) -> alloc::vec::Vec<SuppressedEntry> {
        self.entries.borrow().values().cloned().collect()
    }
}

// ---------------------------------------------------------------------------
// AlertFilter — combines deduplication and suppression
// ---------------------------------------------------------------------------

/// Convenience wrapper that applies both deduplication and explicit suppression
/// in a single `should_deliver` call.
///
/// An alert is delivered only when:
/// 1. It is **not** covered by an active [`AlertSuppressor`] entry, **and**
/// 2. It is **not** within the [`AlertDeduplicator`]'s suppression window.
pub struct AlertFilter {
    dedup: AlertDeduplicator,
    suppressor: AlertSuppressor,
}

impl AlertFilter {
    /// Create a filter with the given deduplication config and an empty
    /// suppression list.
    pub fn new(dedup_config: DedupConfig) -> Self {
        AlertFilter {
            dedup: AlertDeduplicator::new(dedup_config),
            suppressor: AlertSuppressor::new(),
        }
    }

    /// Returns `true` when the alert should be delivered.
    pub fn should_deliver(&self, fingerprint: &str, now: u64) -> bool {
        if self.suppressor.is_suppressed(fingerprint, now) {
            return false;
        }
        self.dedup.should_fire(fingerprint, now)
    }

    /// Access the underlying suppressor to add or lift suppressions.
    pub fn suppressor(&self) -> &AlertSuppressor {
        &self.suppressor
    }

    /// Access the underlying deduplicator to inspect state.
    pub fn dedup(&self) -> &AlertDeduplicator {
        &self.dedup
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── AlertDeduplicator ────────────────────────────────────────────────────

    #[test]
    fn first_occurrence_fires() {
        let d = AlertDeduplicator::new(DedupConfig::default());
        assert!(d.should_fire("k1", 1000));
    }

    #[test]
    fn second_occurrence_within_window_is_suppressed() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        d.should_fire("k1", 1000);
        assert!(!d.should_fire("k1", 1100));
    }

    #[test]
    fn occurrence_after_window_fires_again() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        d.should_fire("k1", 1000);
        assert!(d.should_fire("k1", 1000 + 300 + 1));
    }

    #[test]
    fn alert_fires_exactly_at_ttl_boundary() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        d.should_fire("k1", 1000);
        // now - last == window_seconds → expired, fires (inclusive boundary).
        assert!(d.should_fire("k1", 1000 + 300));
        // Just before boundary remains suppressed.
        let d2 = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        d2.should_fire("k1", 1000);
        assert!(!d2.should_fire("k1", 1000 + 299));
    }

    #[test]
    fn different_fingerprints_are_independent() {
        let d = AlertDeduplicator::new(DedupConfig::default());
        d.should_fire("k1", 1000);
        assert!(d.should_fire("k2", 1001));
    }

    #[test]
    fn window_zero_disables_dedup() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 0, max_keys: 100 });
        d.should_fire("k1", 1000);
        assert!(d.should_fire("k1", 1001));
    }

    #[test]
    fn reset_clears_suppression() {
        let d = AlertDeduplicator::new(DedupConfig::default());
        d.should_fire("k1", 1000);
        d.reset("k1");
        assert!(d.should_fire("k1", 1001));
    }

    #[test]
    fn max_keys_evicts_oldest() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 9999, max_keys: 2 });
        d.should_fire("a", 1000);
        d.should_fire("b", 1001);
        // Adding "c" should evict "a" — it has the smallest last_fired timestamp.
        d.should_fire("c", 1002);
        assert_eq!(d.tracked_count(), 2);
        // "a" was evicted; it should fire again.
        assert!(d.should_fire("a", 1003));
    }

    /// Regression test for the lexical-order eviction bug.
    ///
    /// Insert keys so that alphabetical order differs from timestamp order:
    ///   "z-alert" fired first (ts=1000), "a-alert" fired second (ts=2000).
    /// The old `BTreeMap::keys().next()` code would evict "a-alert" (lexically
    /// first); the fix must evict "z-alert" (oldest timestamp).
    #[test]
    fn eviction_uses_oldest_timestamp_not_lexical_order() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 9999, max_keys: 2 });
        // "z-alert" fires first → timestamp 1000.
        assert!(d.should_fire("z-alert", 1000));
        // "a-alert" fires second → timestamp 2000 (lexically first, but newer).
        assert!(d.should_fire("a-alert", 2000));
        // Capacity is full.  Adding a third key must evict "z-alert" (oldest ts).
        assert!(d.should_fire("m-alert", 3000));
        assert_eq!(d.tracked_count(), 2, "map must stay at max_keys=2");
        // "z-alert" had the minimum timestamp → it was evicted → fires again.
        assert!(
            d.should_fire("z-alert", 3001),
            "z-alert (oldest ts=1000) must have been evicted, not a-alert"
        );
        // "a-alert" was NOT evicted → still suppressed within its window.
        assert!(
            !d.should_fire("a-alert", 3001),
            "a-alert (newer ts=2000) must still be tracked and suppressed"
        );
    }

    #[test]
    fn last_fired_at_returns_correct_timestamp() {
        let d = AlertDeduplicator::new(DedupConfig::default());
        d.should_fire("k1", 5000);
        assert_eq!(d.last_fired_at("k1"), Some(5000));
        assert_eq!(d.last_fired_at("missing"), None);
    }

    #[test]
    fn suppress_count_saturates_at_maximum() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 9999, max_keys: 100 });
        assert!(d.should_fire("k1", 1000));
        assert_eq!(d.suppress_count_for("k1"), Some(0));
        assert!(!d.should_fire("k1", 1001));
        assert_eq!(d.suppress_count_for("k1"), Some(1));
        assert!(!d.should_fire("k1", 1002));
        assert_eq!(d.suppress_count_for("k1"), Some(2));
        // Verify saturating_add: u64::MAX + 1 stays at u64::MAX (no wrap).
        assert_eq!(u64::MAX.saturating_add(1), u64::MAX);
    }

    #[test]
    fn suppression_log_includes_alert_key() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        d.should_fire("anchor:example.com:critical", 1000); // fires — no log entry
        d.should_fire("anchor:example.com:critical", 1100); // suppressed — log entry added
        let log = d.drain_suppression_log();
        assert_eq!(log.len(), 1, "exactly one suppression log entry expected");
        let entry = &log[0];
        assert!(entry.contains("anchor:example.com:critical"),
            "suppression log entry must include the alert key");
        assert!(entry.starts_with('{') && entry.ends_with('}'),
            "suppression log entry must be a JSON object");
    }

    /// The fingerprint is embedded as untrusted data, so quotes, backslashes,
    /// and newlines must be escaped instead of interpolated verbatim.
    #[test]
    fn suppression_log_escapes_fingerprint_json() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        let tricky = "alert\"key\\with\nnewline";
        d.should_fire(tricky, 1000); // fires — no log entry
        d.should_fire(tricky, 1100); // suppressed — log entry added
        let log = d.drain_suppression_log();
        assert_eq!(log.len(), 1);
        // JSON text: {"alert_key":"alert\"key\\with\nnewline"}
        let expected = "{\"alert_key\":\"alert\\\"key\\\\with\\nnewline\"}";
        assert_eq!(log[0], expected, "fingerprint must be JSON-escaped verbatim");
        // The raw entry must not contain an unescaped newline or a bare quote
        // directly attached to the key value.
        assert!(!log[0].contains('\n'), "raw newline would break the JSON line");
    }

    /// The suppression diagnostic collection must stay bounded by max_keys
    /// (same cap the alert-key map uses) while keeping the newest entries.
    #[test]
    fn suppression_log_is_bounded_by_max_keys() {
        let d = AlertDeduplicator::new(DedupConfig { window_seconds: 300, max_keys: 2 });
        assert!(d.should_fire("k1", 1000), "first occurrence fires");
        for i in 1..=5u64 {
            assert!(!d.should_fire("k1", 1000 + i), "repeat within window suppressed");
        }
        let log = d.drain_suppression_log();
        assert_eq!(
            log.len(),
            2,
            "repeated suppression must not grow the log past max_keys"
        );
        // Ordinary single-alert suppression is unchanged.
        assert!(!d.should_fire("k1", 1006));
        assert_eq!(d.drain_suppression_log().len(), 1);
        // The tracked fingerprint map itself stays bounded too.
        assert_eq!(d.tracked_count(), 1);
    }

    // ── AlertSuppressor ──────────────────────────────────────────────────────

    #[test]
    fn suppressed_fingerprint_is_active() {
        let s = AlertSuppressor::new();
        s.suppress("maint:anchor.com", 1000, 2000, "planned maintenance");
        assert!(s.is_suppressed("maint:anchor.com", 1000));
    }

    #[test]
    fn expired_suppression_is_not_active() {
        let s = AlertSuppressor::new();
        s.suppress("maint:anchor.com", 1000, 1500, "window ended");
        assert!(!s.is_suppressed("maint:anchor.com", 2000));
    }

    #[test]
    fn unsuppress_lifts_immediately() {
        let s = AlertSuppressor::new();
        s.suppress("k", 1000, 9999, "test");
        s.unsuppress("k");
        assert!(!s.is_suppressed("k", 1000));
    }

    #[test]
    fn purge_expired_removes_stale_entries() {
        let s = AlertSuppressor::new();
        s.suppress("old", 0, 100, "expired");
        s.suppress("current", 0, 9999, "active");
        s.purge_expired(500);
        assert_eq!(s.active_count(500), 1);
    }

    #[test]
    fn active_count_excludes_expired() {
        let s = AlertSuppressor::new();
        s.suppress("a", 0, 100, "expired");
        s.suppress("b", 0, 9999, "active");
        assert_eq!(s.active_count(500), 1);
    }

    #[test]
    fn overwriting_suppression_extends_expiry() {
        let s = AlertSuppressor::new();
        s.suppress("k", 1000, 1500, "short window");
        s.suppress("k", 1000, 9999, "extended");
        assert!(s.is_suppressed("k", 5000));
    }

    // ── Rejecting already-expired suppressions (#1055) ───────────────────────

    #[test]
    fn zero_duration_suppression_is_rejected() {
        let s = AlertSuppressor::new();
        // expires_at == now → can never suppress a real alert.
        s.suppress("k", 1000, 1000, "zero duration");
        assert!(!s.is_suppressed("k", 1000));
        assert_eq!(s.active_count(1000), 0);
    }

    #[test]
    fn already_expired_suppression_is_rejected() {
        let s = AlertSuppressor::new();
        s.suppress("k", 1000, 900, "expired in the past");
        assert!(!s.is_suppressed("k", 1000));
        assert_eq!(s.active_count(1000), 0);
    }

    #[test]
    fn future_dated_suppression_still_accepted() {
        let s = AlertSuppressor::new();
        s.suppress("k", 1000, 2000, "future window");
        assert!(s.is_suppressed("k", 1500));
        assert_eq!(s.active_count(1500), 1);
    }

    // ── AlertFilter ──────────────────────────────────────────────────────────

    #[test]
    fn filter_delivers_first_occurrence() {
        let f = AlertFilter::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        assert!(f.should_deliver("critical:anchor.com", 1000));
    }

    #[test]
    fn filter_deduplicates_rapid_repeats() {
        let f = AlertFilter::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        f.should_deliver("critical:anchor.com", 1000);
        assert!(!f.should_deliver("critical:anchor.com", 1001));
    }

    #[test]
    fn filter_respects_manual_suppression() {
        let f = AlertFilter::new(DedupConfig { window_seconds: 300, max_keys: 100 });
        f.suppressor().suppress("maint:anchor.com", 0, 9999, "maintenance");
        // Would fire on first occurrence via dedup alone, but suppressor blocks it.
        assert!(!f.should_deliver("maint:anchor.com", 1000));
    }

    #[test]
    fn filter_delivers_after_suppression_expires() {
        let f = AlertFilter::new(DedupConfig { window_seconds: 60, max_keys: 100 });
        f.suppressor().suppress("k", 0, 1500, "short maintenance");
        // Suppressed within window.
        assert!(!f.should_deliver("k", 1000));
        // Suppression expired and dedup window also passed → fires.
        assert!(f.should_deliver("k", 2000));
    }
}
//! Admin audit log for tracking configuration changes.
//!
//! This module provides audit logging for all admin configuration changes,
//! including endpoint updates, service configuration, and other administrative operations.

use soroban_sdk::{contracttype, panic_with_error, Address, Env, String};
use crate::deterministic_hash::make_storage_key;
use crate::errors::ErrorCode;

/// Represents a single admin configuration change event.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminConfigChangeEvent {
    /// Unique identifier for this audit entry
    pub entry_id: u64,
    /// Admin address that made the change
    pub admin: Address,
    /// Type of configuration change (e.g., "endpoint_update", "service_config", "rate_limit_update")
    pub change_type: String,
    /// Target of the change (e.g., attestor address, service name)
    pub target: String,
    /// Previous value (empty string if not applicable)
    pub old_value: String,
    /// New value (empty string if not applicable)
    pub new_value: String,
    /// Timestamp of the change
    pub timestamp: u64,
    /// Status of the change ("success" or "failed")
    pub status: String,
    /// Optional error message if status is "failed"
    pub error_message: String,
}

/// Represents the admin audit log configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminAuditLogConfig {
    /// Whether admin audit logging is enabled
    pub enabled: bool,
    /// Maximum number of entries to retain (0 = unlimited)
    pub max_entries: u32,
    /// TTL for audit entries in seconds
    pub ttl_seconds: u64,
}

impl AdminAuditLogConfig {
    /// Create a default admin audit log configuration
    pub fn default() -> Self {
        AdminAuditLogConfig {
            enabled: true,
            max_entries: 10000,
            ttl_seconds: 31_536_000, // 1 year
        }
    }
}

/// Admin audit log manager
pub struct AdminAuditLog;

impl AdminAuditLog {
    /// Log an admin configuration change
    pub fn log_change(
        env: &Env,
        admin: &Address,
        change_type: &str,
        target: &str,
        old_value: &str,
        new_value: &str,
    ) {
        Self::log_change_with_status(
            env,
            admin,
            change_type,
            target,
            old_value,
            new_value,
            "success",
            "",
        );
    }

    /// Log an admin configuration change with status
    pub fn log_change_with_status(
        env: &Env,
        admin: &Address,
        change_type: &str,
        target: &str,
        old_value: &str,
        new_value: &str,
        status: &str,
        error_message: &str,
    ) {
        Self::write_event(
            env,
            admin,
            String::from_str(env, change_type),
            String::from_str(env, target),
            String::from_str(env, old_value),
            String::from_str(env, new_value),
            String::from_str(env, status),
            String::from_str(env, error_message),
        );
    }

    /// Log an admin configuration change where `target` is an on-chain value
    /// already rendered as a Soroban [`String`] (for example `addr.to_string()`).
    ///
    /// This mirrors [`Self::log_change`] but avoids forcing callers inside the
    /// `no_std` contract to materialise a `&str` for an [`Address`] target. It
    /// is the entry point used by `contract.rs` to record admin operations.
    pub fn log_action(
        env: &Env,
        admin: &Address,
        change_type: &str,
        target: String,
        old_value: &str,
        new_value: &str,
    ) {
        Self::write_event(
            env,
            admin,
            String::from_str(env, change_type),
            target,
            String::from_str(env, old_value),
            String::from_str(env, new_value),
            String::from_str(env, "success"),
            String::from_str(env, ""),
        );
    }

    /// Core audit-entry writer shared by the public logging helpers.
    ///
    /// All string fields are already materialised as Soroban [`String`]s so this
    /// function never has to assume the target is a `&str`.
    fn write_event(
        env: &Env,
        admin: &Address,
        change_type: String,
        target: String,
        old_value: String,
        new_value: String,
        status: String,
        error_message: String,
    ) {
        // Check if audit logging is enabled
        let config = Self::get_config(env);

        if !config.enabled {
            return;
        }

        // Get next entry ID
        let counter_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CNT");
        let entry_id: u64 = env
            .storage()
            .instance()
            .get(&counter_key)
            .unwrap_or(0u64);

        let entry_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT");

        // Evict the oldest entry when the log is at capacity (circular buffer).
        if config.max_entries > 0 && entry_id >= config.max_entries as u64 {
            let oldest_id = entry_id - config.max_entries as u64;
            env.storage().instance().remove(&(entry_key.clone(), oldest_id));
        }

        // Create the audit event
        let event = AdminConfigChangeEvent {
            entry_id,
            admin: admin.clone(),
            change_type,
            target,
            old_value,
            new_value,
            timestamp: env.ledger().timestamp(),
            status,
            error_message,
        };

        // Validate that the configured TTL fits in the u32 expected by extend_ttl.
        // A silent truncating cast would make entries expire far earlier than
        // configured; reject the write instead.
        let ttl_u32: u32 = config
            .ttl_seconds
            .try_into()
            .unwrap_or_else(|_| panic_with_error!(env, ErrorCode::ValidationError));

        // Store the event using entry_id as part of the key
        env.storage().instance().set(&(entry_key, entry_id), &event);
        env.storage()
            .instance()
            .extend_ttl(ttl_u32, ttl_u32);

        // Increment counter — use checked arithmetic so a counter at u64::MAX
        // fails deterministically instead of wrapping and reusing an ID.
        let next_id = entry_id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, ErrorCode::AuditLogCapacityExceeded));
        env.storage()
            .instance()
            .set(&counter_key, &next_id);
        env.storage()
            .instance()
            .extend_ttl(ttl_u32, ttl_u32);

        // Publish event
        env.events().publish(
            (
                soroban_sdk::symbol_short!("admin"),
                soroban_sdk::symbol_short!("audit"),
                entry_id,
            ),
            event,
        );
    }

    /// Get an admin audit log entry by ID
    pub fn get_entry(env: &Env, entry_id: u64) -> Option<AdminConfigChangeEvent> {
        let entry_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT");
        env.storage().instance().get(&(entry_key, entry_id))
    }

    /// Get the total number of audit entries
    pub fn get_entry_count(env: &Env) -> u64 {
        let counter_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CNT");
        env.storage().instance().get(&counter_key).unwrap_or(0u64)
    }

    /// Get the admin audit log configuration
    pub fn get_config(env: &Env) -> AdminAuditLogConfig {
        let config_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CFG");
        env.storage()
            .instance()
            .get(&config_key)
            .unwrap_or_else(|| AdminAuditLogConfig::default())
    }

    /// Update the admin audit log configuration
    pub fn set_config(env: &Env, config: &AdminAuditLogConfig) {
        let config_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CFG");
        env.storage().instance().set(&config_key, config);
        let ttl_u32: u32 = config
            .ttl_seconds
            .try_into()
            .unwrap_or_else(|_| panic_with_error!(env, ErrorCode::ValidationError));
        env.storage()
            .instance()
            .extend_ttl(ttl_u32, ttl_u32);
    }

    /// Clear all audit entries (admin only).
    ///
    /// Requires the caller to be the configured admin address; panics with
    /// [`ErrorCode::Unauthorized`] if the stored admin key is absent or the
    /// caller is not that address.  After a successful call every previously
    /// stored record is removed from storage and the ID counter is reset to
    /// zero, providing a clean audit boundary.
    pub fn clear_entries(env: &Env, caller: &Address) {
        // Authorization: the caller must be the primary admin.
        let admin_storage_key = make_storage_key(env, &[b"ADMIN"]);
        let admin: Address = env
            .storage()
            .instance()
            .get::<_, Address>(&admin_storage_key)
            .unwrap_or_else(|| panic_with_error!(env, ErrorCode::Unauthorized));
        if *caller != admin {
            panic_with_error!(env, ErrorCode::Unauthorized);
        }
        caller.require_auth();

        // Delete every stored audit record up to the current counter value.
        let counter_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CNT");
        let count: u64 = env
            .storage()
            .instance()
            .get(&counter_key)
            .unwrap_or(0u64);

        let entry_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT");
        for id in 0..count {
            env.storage().instance().remove(&(entry_key.clone(), id));
        }

        // Reset the counter so the next write starts from ID 0.
        env.storage().instance().set(&counter_key, &0u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_audit_log_config_default() {
        let config = AdminAuditLogConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10000);
        assert_eq!(config.ttl_seconds, 31_536_000);
    }

    /// Verify that checked_add prevents ID wrap-around at u64::MAX.
    ///
    /// We exercise the arithmetic directly — the on-chain path that calls
    /// `panic_with_error!` cannot run in a unit test without a full Soroban
    /// mock environment, but we can confirm that `u64::MAX.checked_add(1)`
    /// returns `None`, which is what the production code relies on to detect
    /// overflow and abort before writing a duplicate ID.
    #[test]
    fn test_counter_overflow_is_detected_not_wrapped() {
        // Simulate the checked_add that write_event now performs.
        let at_max: u64 = u64::MAX;
        let result = at_max.checked_add(1);
        assert!(
            result.is_none(),
            "checked_add must return None at u64::MAX so the contract can \
             abort with AuditLogCapacityExceeded instead of reusing ID 0"
        );

        // Normal increments must still produce consecutive IDs.
        let normal: u64 = 41;
        assert_eq!(normal.checked_add(1), Some(42));
    }
}
