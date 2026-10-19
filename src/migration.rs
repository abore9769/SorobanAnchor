//! Versioned migration framework for on-chain storage upgrades.
//!
//! ## Design goals
//!
//! 1. **Explicit version checks** — every migration validates that the stored
//!    schema version exactly matches its precondition before touching storage.
//! 2. **Rollback-safe writes** — migrations read, transform, and write in a
//!    single logical step; the stored version is only advanced after all data
//!    writes succeed.
//! 3. **Recorded history** — each applied migration appends an entry to a
//!    persistent log so operators can audit exactly which migrations have run.
//! 4. **Forward-safety** — the framework refuses to apply a migration whose
//!    target version exceeds the highest version the current WASM binary
//!    understands, preventing half-applied upgrades.
//!//! Lightweight in-process metrics for host-side anchor operations.
//!
//! Production anchors need operational visibility: success rates, retry
//! volume, failure counts and resource usage. This module provides a small
//! dependency-free metrics layer that the host-side code paths (HTTP
//! requests, retries, webhook delivery) attach to via `*_metered` wrapper
//! functions.
//!
//! # Design
//!
//! * **No external crates.** Counters and gauges are plain `u64` values in
//!   `BTreeMap`s guarded by `RefCell`, mirroring the interior-mutability
//!   pattern already used by webhook delivery. This keeps the crate free of
//!   a metrics-backend dependency; a snapshot can be exported to any backend
//!   by the embedding application.
//! * **Saturating arithmetic.** The workspace builds with
//!   `overflow-checks = true` in release, so all updates saturate instead of
//!   panicking at the counter ceiling.
//! * **Namespacing.** [`MetricsRegistry::with_namespace`] prefixes every
//!   metric name (`<namespace>.<name>`), matching the `metrics_namespace`
//!   field that runtime configs already declare under `monitoring`.
//!
//! # Example
//!
//! ```
//! use anchorkit::metrics::{names, MetricsRegistry};
//!
//! let metrics = MetricsRegistry::new();
//! metrics.record_call(names::CONTRACT_CALL, true);
//! metrics.record_call(names::CONTRACT_CALL, false);
//!
//! assert_eq!(metrics.counter(&names::calls(names::CONTRACT_CALL)), 2);
//! assert_eq!(metrics.counter(&names::failures(names::CONTRACT_CALL)), 1);
//! ```

#[cfg(feature = "std")]
extern crate std;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use core::cell::RefCell;

// ---------------------------------------------------------------------------
// Metric names
// ---------------------------------------------------------------------------

/// Canonical metric names emitted by the built-in `*_metered` wrappers.
///
/// Free functions build per-operation names (`<operation>.calls`,
/// `<operation>.retry.attempts`, ...) so unrelated operations never share a
/// counter.
pub mod names {
    use alloc::format;
    use alloc::string::String;

    /// Total outbound HTTP POSTs started via `post_with_options_metered`.
    pub const HTTP_REQUESTS: &str = "http.requests";
    /// HTTP POSTs that completed with a status below 400.
    pub const HTTP_SUCCESSES: &str = "http.successes";
    /// HTTP POSTs that completed with a status of 400 or above.
    pub const HTTP_ERROR_RESPONSES: &str = "http.error_responses";
    /// HTTP POSTs that failed at the transport layer (no status received).
    pub const HTTP_TRANSPORT_ERRORS: &str = "http.transport_errors";

    /// Webhook delivery operations started via `deliver_webhook_metered`.
    pub const WEBHOOK_DELIVERIES: &str = "webhook.deliveries";
    /// Individual webhook POST attempts (including retries).
    pub const WEBHOOK_ATTEMPTS: &str = "webhook.attempts";
    /// Webhook deliveries that eventually succeeded.
    pub const WEBHOOK_SUCCESSES: &str = "webhook.successes";
    /// Webhook deliveries that exhausted all attempts and failed.
    pub const WEBHOOK_FAILURES: &str = "webhook.failures";
    /// Entries appended to the dead-letter queue.
    pub const WEBHOOK_DLQ_ENTRIES: &str = "webhook.dlq_entries";
    /// Gauge: total entries currently held across all DLQ keys.
    pub const WEBHOOK_DLQ_DEPTH: &str = "webhook.dlq_depth";

    /// Operation name for Soroban contract invocations recorded through
    /// [`super::MetricsRegistry::record_call`].
    pub const CONTRACT_CALL: &str = "contract_call";

    /// `<operation>.calls` — total invocations of an operation.
    pub fn calls(operation: &str) -> String {
        format!("{operation}.calls")
    }

    /// `<operation>.successes` — invocations that succeeded.
    pub fn successes(operation: &str) -> String {
        format!("{operation}.successes")
    }

    /// `<operation>.failures` — invocations that failed.
    pub fn failures(operation: &str) -> String {
        format!("{operation}.failures")
    }

    /// `<operation>.retry.attempts` — closure executions inside a retry loop.
    pub fn retry_attempts(operation: &str) -> String {
        format!("{operation}.retry.attempts")
    }

    /// `<operation>.retry.backoffs` — sleeps between attempts (i.e. actual retries).
    pub fn retry_backoffs(operation: &str) -> String {
        format!("{operation}.retry.backoffs")
    }

    /// `<operation>.retry.successes` — retry loops that returned `Ok`.
    pub fn retry_successes(operation: &str) -> String {
        format!("{operation}.retry.successes")
    }

    /// `<operation>.retry.failures` — retry loops that gave up with `Err`.
    pub fn retry_failures(operation: &str) -> String {
        format!("{operation}.retry.failures")
    }
}

// ---------------------------------------------------------------------------
// Latency summary
// ---------------------------------------------------------------------------

/// Aggregated latency observations for one operation.
///
/// Stores count/total/max rather than raw samples so memory stays bounded
/// regardless of call volume.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatencySummary {
    /// Number of latency observations recorded.
    pub count: u64,
    /// Sum of all observed latencies in milliseconds.
    pub total_ms: u64,
    /// Largest single observed latency in milliseconds.
    pub max_ms: u64,
}

impl LatencySummary {
    /// Mean observed latency in milliseconds (0 when nothing was recorded).
    pub fn avg_ms(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.total_ms / self.count
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// Point-in-time copy of every metric held by a [`MetricsRegistry`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetricsSnapshot {
    /// Monotonically increasing event counts, keyed by metric name.
    pub counters: BTreeMap<String, u64>,
    /// Last-written point-in-time values, keyed by metric name.
    pub gauges: BTreeMap<String, u64>,
    /// Aggregated latency observations, keyed by metric name.
    pub latencies: BTreeMap<String, LatencySummary>,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// In-process registry of counters, gauges and latency summaries.
///
/// Uses `RefCell` interior mutability so instrumentation can happen behind
/// `&self` inside `Fn` closures (the same pattern webhook delivery uses to
/// capture per-attempt state). The registry is intentionally not `Sync`;
/// host-side anchor operations in this crate are single-threaded blocking
/// calls.
#[derive(Debug, Default)]
pub struct MetricsRegistry {
    namespace: Option<String>,
    counters: RefCell<BTreeMap<String, u64>>,
    gauges: RefCell<BTreeMap<String, u64>>,
    latencies: RefCell<BTreeMap<String, LatencySummary>>,
}

impl MetricsRegistry {
    /// Create an empty registry with no namespace prefix.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty registry whose metric names are prefixed with
    /// `<namespace>.` — e.g. `anchorkit.fiat_ramp.http.requests`.
    pub fn with_namespace(namespace: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            ..Self::default()
        }
    }

    /// The namespace prefix, if one was configured.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    fn qualify(&self, name: &str) -> String {
        match &self.namespace {
            Some(ns) => format!("{ns}.{name}"),
            None => name.to_string(),
        }
    }

    /// Increment the counter `name` by 1.
    pub fn incr(&self, name: &str) {
        self.incr_by(name, 1);
    }

    /// Increment the counter `name` by `delta`, saturating at `u64::MAX`.
    pub fn incr_by(&self, name: &str, delta: u64) {
        let key = self.qualify(name);
        let mut counters = self.counters.borrow_mut();
        let entry = counters.entry(key).or_insert(0);
        *entry = entry.saturating_add(delta);
    }

    /// Current value of the counter `name` (0 if it was never incremented).
    pub fn counter(&self, name: &str) -> u64 {
        self.counters
            .borrow()
            .get(&self.qualify(name))
            .copied()
            .unwrap_or(0)
    }

    /// Set the gauge `name` to `value`, replacing any previous value.
    pub fn set_gauge(&self, name: &str, value: u64) {
        self.gauges.borrow_mut().insert(self.qualify(name), value);
    }

    /// Current value of the gauge `name`, or `None` if it was never set.
    pub fn gauge(&self, name: &str) -> Option<u64> {
        self.gauges.borrow().get(&self.qualify(name)).copied()
    }

    /// Record one latency observation (milliseconds) for `name`.
    pub fn observe_latency_ms(&self, name: &str, elapsed_ms: u64) {
        let key = self.qualify(name);
        let mut latencies = self.latencies.borrow_mut();
        let summary = latencies.entry(key).or_default();
        summary.count = summary.count.saturating_add(1);
        summary.total_ms = summary.total_ms.saturating_add(elapsed_ms);
        if elapsed_ms > summary.max_ms {
            summary.max_ms = elapsed_ms;
        }
    }

    /// Aggregated latency for `name`, or `None` if nothing was observed.
    pub fn latency(&self, name: &str) -> Option<LatencySummary> {
        self.latencies.borrow().get(&self.qualify(name)).cloned()
    }

    /// Record one invocation of `operation`, bumping `<operation>.calls`
    /// plus `<operation>.successes` or `<operation>.failures`.
    ///
    /// Use [`names::CONTRACT_CALL`] as the operation to count Soroban
    /// contract invocations made by an embedding application.
    pub fn record_call(&self, operation: &str, success: bool) {
        self.incr(&names::calls(operation));
        if success {
            self.incr(&names::successes(operation));
        } else {
            self.incr(&names::failures(operation));
        }
    }

    /// Copy every metric into an owned [`MetricsSnapshot`].
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            counters: self.counters.borrow().clone(),
            gauges: self.gauges.borrow().clone(),
            latencies: self.latencies.borrow().clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime-config integration (std only)
// ---------------------------------------------------------------------------

#[cfg(feature = "std")]
impl MetricsRegistry {
    /// Build a registry from the `monitoring` section of a runtime config.
    ///
    /// Returns `Some` only when `enable_metrics` is explicitly `true`
    /// (metrics are opt-in). The registry adopts `metrics_namespace` when
    /// one is configured. Previously these config fields were parsed but
    /// never read by any code path.
    pub fn from_monitoring_config(
        monitoring: Option<&crate::config::MonitoringConfig>,
    ) -> Option<Self> {
        let monitoring = monitoring?;
        if !monitoring.enable_metrics.unwrap_or(false) {
            return None;
        }
        Some(match monitoring.metrics_namespace.as_deref() {
            Some(ns) => Self::with_namespace(ns),
            None => Self::new(),
        })
    }
}

/// Run `f`, recording its wall-clock duration against `name` (std only).
///
/// Returns whatever `f` returns; the latency is recorded whether the inner
/// operation logically succeeded or not.
#[cfg(feature = "std")]
pub fn time_operation<T>(metrics: &MetricsRegistry, name: &str, f: impl FnOnce() -> T) -> T {
    let start = std::time::Instant::now();
    let out = f();
    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    metrics.observe_latency_ms(name, elapsed_ms);
    out
}

//! ## Adding a new migration
//!
//! 1. Increment `LATEST_SCHEMA_VERSION`.
//! 2. Add a new `MigrationStep::V<N>` variant.
//! 3. Implement the migration logic inside `apply_step`.
//! 4. Register the step in `ALL_STEPS`.

use soroban_sdk::{contracttype, Env, String};
use crate::deterministic_hash::make_storage_key;

// ---------------------------------------------------------------------------
// Version constants
// ---------------------------------------------------------------------------

/// The initial schema version written by `initialize()`.
pub const SCHEMA_V1: u32 = 1;

/// Schema V2 adds `routing_reason` to [`Quote`](crate::contract::Quote) records.
pub const SCHEMA_V2: u32 = 2;

/// The highest schema version this binary understands.  
/// `migrate()` rejects any target version greater than this.
pub const LATEST_SCHEMA_VERSION: u32 = SCHEMA_V2;

// ---------------------------------------------------------------------------
// History retention
// ---------------------------------------------------------------------------

/// Retention period (in ledgers, ~90 days at 5 s/ledger) for migration history
/// entries — the counter and every [`MigrationRecord`].
///
/// Matches the contract-wide `PERSISTENT_TTL` convention and is longer than
/// the instance TTL that keeps the schema version alive, so the audit trail
/// never lapses before the version it explains. The TTL is (re)applied when a
/// record is written and every time the history is read, so an actively
/// audited history stays live for at least this long after its last access.
pub const MIGRATION_HISTORY_TTL: u32 = 1_555_200;

// ---------------------------------------------------------------------------
// Migration step registry
// ---------------------------------------------------------------------------

/// An individual, idempotent migration step.
///
/// Each variant maps to exactly one schema version bump.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MigrationStep {
    /// V1 → V2: add `routing_reason` field to all stored [`Quote`] records.
    ToV2,
}

/// All known migration steps in application order.
pub const ALL_STEPS: &[MigrationStep] = &[
    MigrationStep::ToV2,
];

impl MigrationStep {
    /// The schema version this step requires as a *precondition* (current stored version).
    pub fn required_from(self) -> u32 {
        match self {
            MigrationStep::ToV2 => SCHEMA_V1,
        }
    }

    /// The schema version produced after this step completes successfully.
    pub fn produces(self) -> u32 {
        match self {
            MigrationStep::ToV2 => SCHEMA_V2,
        }
    }

    /// A stable, human-readable label for audit log entries.
    pub fn label(self) -> &'static str {
        match self {
            MigrationStep::ToV2 => "quotes_v1_to_v2",
        }
    }
}

// ---------------------------------------------------------------------------
// Migration history record
// ---------------------------------------------------------------------------

/// Persistent record of a single applied migration.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MigrationRecord {
    /// Which schema version was in effect before this migration ran.
    pub from_version: u32,
    /// Which schema version is in effect after this migration completed.
    pub to_version: u32,
    /// Ledger timestamp when the migration completed.
    pub applied_at: u64,
    /// The ledger sequence number when applied.
    pub applied_at_ledger: u32,
    /// Human-readable label identifying the migration step.
    pub label: String,
}

// ---------------------------------------------------------------------------
// Storage key helpers
// ---------------------------------------------------------------------------

fn migration_count_key(env: &Env) -> soroban_sdk::BytesN<32> {
    make_storage_key(env, &[b"MIG_CNT"])
}

fn migration_record_key(env: &Env, idx: u32) -> soroban_sdk::BytesN<32> {
    make_storage_key(env, &[b"MIG_REC", &idx.to_be_bytes()])
}

pub(crate) fn schema_version_key(env: &Env) -> soroban_sdk::BytesN<32> {
    make_storage_key(env, &[b"SCHEMAVER"])
}

fn extend_history_ttl(env: &Env, key: &soroban_sdk::BytesN<32>) {
    env.storage()
        .persistent()
        .extend_ttl(key, MIGRATION_HISTORY_TTL, MIGRATION_HISTORY_TTL);
}

// ---------------------------------------------------------------------------
// Public API used by the contract layer
// ---------------------------------------------------------------------------

/// Read the currently stored schema version.
/// Returns `0` if `initialize()` has never been called.
pub fn current_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&schema_version_key(env))
        .unwrap_or(0)
}

/// Stamp the initial schema version. Called once by `initialize()`.
///
/// This is **not** a general-purpose setter: the only legal transition is
/// uninitialized (`0`) → [`SCHEMA_V1`]. Every later version change must go
/// through [`validate_migration`] + [`commit_version`] so that the registered
/// migration steps actually run and are recorded in history.
///
/// Returns [`MigrationError::IllegalVersionTransition`] without touching
/// storage for any other request (jumps, downgrades, re-initialization).
pub fn set_version(env: &Env, version: u32) -> Result<(), MigrationError> {
    if current_version(env) != 0 || version != SCHEMA_V1 {
        return Err(MigrationError::IllegalVersionTransition);
    }
    env.storage()
        .instance()
        .set(&schema_version_key(env), &version);
    Ok(())
}

/// Advance the stored schema version and append a [`MigrationRecord`] to the
/// persistent history log.
///
/// **Must only be called** after all data writes for the step have succeeded.
/// The caller (the contract's `migrate()` function) is responsible for the
/// data-transformation work; this function only commits the version bump and
/// writes the audit record.
///
/// Returns [`MigrationError::HistoryCounterExhausted`] without writing anything
/// if the history counter is already at `u32::MAX`, since advancing it would
/// wrap and overwrite record 0.
pub fn commit_version(env: &Env, from: u32, to: u32, label: &str) -> Result<(), MigrationError> {
    let cnt_key = migration_count_key(env);
    let idx: u32 = env
        .storage()
        .persistent()
        .get(&cnt_key)
        .unwrap_or(0u32);
    let next_idx = idx
        .checked_add(1)
        .ok_or(MigrationError::HistoryCounterExhausted)?;

    // Advance the stored version.
    env.storage()
        .instance()
        .set(&schema_version_key(env), &to);

    // Append history record.

    let record = MigrationRecord {
        from_version: from,
        to_version: to,
        applied_at: env.ledger().timestamp(),
        applied_at_ledger: env.ledger().sequence(),
        label: String::from_str(env, label),
    };
    let rec_key = migration_record_key(env, idx);
    env.storage().persistent().set(&rec_key, &record);
    extend_history_ttl(env, &rec_key);

    env.storage().persistent().set(&cnt_key, &next_idx);
    extend_history_ttl(env, &cnt_key);
    Ok(())
}

/// Return the total number of migrations recorded in the history log.
///
/// Refreshes the counter's TTL to [`MIGRATION_HISTORY_TTL`] when it exists.
pub fn migration_count(env: &Env) -> u32 {
    let key = migration_count_key(env);
    let count: Option<u32> = env.storage().persistent().get(&key);
    if count.is_some() {
        extend_history_ttl(env, &key);
    }
    count.unwrap_or(0)
}

/// Return a specific migration record by zero-based index, or `None`.
///
/// Refreshes the record's TTL to [`MIGRATION_HISTORY_TTL`] when it exists.
pub fn get_migration_record(env: &Env, idx: u32) -> Option<MigrationRecord> {
    let key = migration_record_key(env, idx);
    let record: Option<MigrationRecord> = env.storage().persistent().get(&key);
    if record.is_some() {
        extend_history_ttl(env, &key);
    }
    record
}

/// Validate that `target_version` is a legal migration target.
///
/// Returns `Ok(step)` with the matching [`MigrationStep`] when:
/// - `target_version > 0`
/// - `target_version > current_version`
/// - `target_version <= LATEST_SCHEMA_VERSION`
/// - a registered step exists whose `required_from` matches `current_version`
///   and whose `produces` matches `target_version`
///
/// Returns `Err(MigrationError)` otherwise.
pub fn validate_migration(
    env: &Env,
    target_version: u32,
) -> Result<MigrationStep, MigrationError> {
    if target_version == 0 {
        return Err(MigrationError::InvalidTargetVersion);
    }
    if target_version > LATEST_SCHEMA_VERSION {
        return Err(MigrationError::VersionTooNew);
    }
    let current = current_version(env);
    if target_version <= current {
        return Err(MigrationError::VersionNotAdvancing);
    }
    // Find a step that bridges current → target.
    for &step in ALL_STEPS {
        if step.required_from() == current && step.produces() == target_version {
            return Ok(step);
        }
    }
    Err(MigrationError::NoStepFound)
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Reason a migration request was rejected.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MigrationError {
    /// `target_version` was 0.
    InvalidTargetVersion,
    /// `target_version` exceeds `LATEST_SCHEMA_VERSION`.
    VersionTooNew,
    /// `target_version` is not greater than the current stored version.
    VersionNotAdvancing,
    /// No registered migration step bridges the current version to the target.
    NoStepFound,
    /// The history counter is at `u32::MAX`; another record would wrap its ID.
    HistoryCounterExhausted,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod migration_tests {
    use super::*;
    use soroban_sdk::testutils::storage::Persistent as _;
    use soroban_sdk::testutils::Ledger as _;

    /// Run `f` inside a registered contract so storage access is permitted.
    fn with_contract(f: impl FnOnce(&Env)) {
        let env = Env::default();
        let id = env.register_contract(None, crate::contract::AnchorKitContract);
        env.as_contract(&id, || f(&env));
    }

    /// Run `f` inside a registered contract so storage access is permitted.
    fn in_contract(env: &Env, f: impl FnOnce()) {
        let id = env.register_contract(None, crate::contract::AnchorKitContract);
        env.as_contract(&id, f);
    }

    /// Write the stored version directly, bypassing `set_version` validation,
    /// so tests can set up arbitrary starting states.
    fn force_version(env: &Env, version: u32) {
        env.storage()
            .instance()
            .set(&schema_version_key(env), &version);
    }

    // -----------------------------------------------------------------------
    // Version constants
    // -----------------------------------------------------------------------

    #[test]
    fn test_latest_schema_version_is_v2() {
        assert_eq!(LATEST_SCHEMA_VERSION, SCHEMA_V2);
        assert_eq!(SCHEMA_V1, 1u32);
        assert_eq!(SCHEMA_V2, 2u32);
    }

    // -----------------------------------------------------------------------
    // current_version / set_version
    // -----------------------------------------------------------------------

    #[test]
    fn test_current_version_default_is_zero() {
        with_contract(|env| {
            assert_eq!(current_version(env), 0);
        });
    }

    #[test]
    fn test_set_version_stores_value() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            assert_eq!(current_version(env), SCHEMA_V1);
        });
    }

    // -----------------------------------------------------------------------
    // validate_migration
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_migration_zero_target_fails() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            assert_eq!(
                validate_migration(env, 0),
                Err(MigrationError::InvalidTargetVersion)
            );
        });
    }

    #[test]
    fn test_validate_migration_version_too_new_fails() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            assert_eq!(
                validate_migration(env, LATEST_SCHEMA_VERSION + 1),
                Err(MigrationError::VersionTooNew)
            );
        });
    }

    #[test]
    fn test_validate_migration_not_advancing_fails() {
        with_contract(|env| {
            set_version(env, SCHEMA_V2);
            // V2 → V2 must fail.
            assert_eq!(
                validate_migration(env, SCHEMA_V2),
                Err(MigrationError::VersionNotAdvancing)
            );
        });
    }

    #[test]
    fn test_validate_migration_v1_to_v2_succeeds() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            let result = validate_migration(env, SCHEMA_V2);
            assert!(result.is_ok(), "v1 → v2 should be valid");
            assert_eq!(result.unwrap(), MigrationStep::ToV2);
        });
    }

    #[test]
    fn test_validate_migration_skipping_step_fails() {
        with_contract(|env| {
            // No step exists from V0 → V2 (skips V1).
            set_version(env, 0);
            let result = validate_migration(env, SCHEMA_V2);
            // There is no registered step from 0 → 2 (only 1 → 2 is registered).
            assert_eq!(result, Err(MigrationError::NoStepFound));
        });
    }

    // -----------------------------------------------------------------------
    // commit_version / migration history
    // -----------------------------------------------------------------------

    #[test]
    fn test_commit_version_advances_stored_version() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            commit_version(env, SCHEMA_V1, SCHEMA_V2, "test_migration").unwrap();
            assert_eq!(current_version(env), SCHEMA_V2);
        });
    }

    #[test]
    fn test_commit_version_appends_history_record() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            assert_eq!(migration_count(env), 0);
            commit_version(env, SCHEMA_V1, SCHEMA_V2, "quotes_v1_to_v2").unwrap();
            assert_eq!(migration_count(env), 1);
        });
    }

    #[test]
    fn test_get_migration_record_returns_correct_data() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            commit_version(env, SCHEMA_V1, SCHEMA_V2, "quotes_v1_to_v2").unwrap();
            let rec = get_migration_record(env, 0).expect("record 0 should exist");
            assert_eq!(rec.from_version, SCHEMA_V1);
            assert_eq!(rec.to_version, SCHEMA_V2);
        });
    }

    #[test]
    fn test_get_migration_record_out_of_range_returns_none() {
        with_contract(|env| {
            assert!(get_migration_record(env, 99).is_none());
        });
    }

    #[test]
    fn test_multiple_commits_all_recorded() {
        with_contract(|env| {
            // Simulate two distinct migrations (using raw set_version for the second).
            set_version(env, SCHEMA_V1);
            commit_version(env, SCHEMA_V1, SCHEMA_V2, "step_a").unwrap();
            // Fake a third version for the second commit without a real step.
            commit_version(env, SCHEMA_V2, 3, "step_b").unwrap();
            assert_eq!(migration_count(env), 2);
            let r0 = get_migration_record(env, 0).unwrap();
            let r1 = get_migration_record(env, 1).unwrap();
            assert_eq!(r0.to_version, SCHEMA_V2);
            assert_eq!(r1.from_version, SCHEMA_V2);
            assert_eq!(r1.to_version, 3);
        });
    }

    #[test]
    fn test_commit_version_uses_last_id_below_max() {
        with_contract(|env| {
            let last = u32::MAX - 1;
            env.storage().persistent().set(&migration_count_key(env), &last);
            set_version(env, SCHEMA_V1);

            commit_version(env, SCHEMA_V1, SCHEMA_V2, "last_slot").unwrap();

            assert_eq!(migration_count(env), u32::MAX);
            let rec = get_migration_record(env, last).expect("record at MAX-1");
            assert_eq!(rec.to_version, SCHEMA_V2);
            assert_eq!(current_version(env), SCHEMA_V2);
        });
    }

    #[test]
    fn test_commit_version_at_max_counter_fails_without_writing() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            commit_version(env, SCHEMA_V1, SCHEMA_V2, "first").unwrap();
            let original = get_migration_record(env, 0).unwrap();
            env.storage().persistent().set(&migration_count_key(env), &u32::MAX);

            assert_eq!(
                commit_version(env, SCHEMA_V2, 3, "overflow"),
                Err(MigrationError::HistoryCounterExhausted)
            );

            // Nothing moved: no version bump, no counter wrap, no new or
            // overwritten record.
            assert_eq!(current_version(env), SCHEMA_V2);
            assert_eq!(migration_count(env), u32::MAX);
            assert!(get_migration_record(env, u32::MAX).is_none());
            let rec0 = get_migration_record(env, 0).unwrap();
            assert_eq!(rec0.label, original.label);
            assert_eq!(rec0.to_version, original.to_version);
        });
    }

    // -----------------------------------------------------------------------
    // History retention
    // -----------------------------------------------------------------------

    #[test]
    fn test_history_ttl_matches_retention_policy_on_commit() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            commit_version(env, SCHEMA_V1, SCHEMA_V2, "quotes_v1_to_v2").unwrap();
            let storage = env.storage().persistent();
            assert_eq!(storage.get_ttl(&migration_record_key(env, 0)), MIGRATION_HISTORY_TTL);
            assert_eq!(storage.get_ttl(&migration_count_key(env)), MIGRATION_HISTORY_TTL);
        });
    }

    #[test]
    fn test_history_survives_past_original_ttl_when_read() {
        with_contract(|env| {
            set_version(env, SCHEMA_V1);
            commit_version(env, SCHEMA_V1, SCHEMA_V2, "quotes_v1_to_v2").unwrap();

            // Read shortly before the write-time TTL would lapse, then advance
            // past where the original TTL ended. Each read refreshes retention.
            let step = MIGRATION_HISTORY_TTL - 10;
            for _ in 0..2 {
                // Stand-in for ordinary contract traffic keeping the instance
                // (and so the schema version) alive.
                env.storage().instance().extend_ttl(MIGRATION_HISTORY_TTL, MIGRATION_HISTORY_TTL);
                env.ledger().with_mut(|l| l.sequence_number += step);
                assert_eq!(migration_count(env), 1);
                let rec = get_migration_record(env, 0).expect("history must not expire");
                assert_eq!(rec.to_version, SCHEMA_V2);
                let storage = env.storage().persistent();
                assert_eq!(storage.get_ttl(&migration_record_key(env, 0)), MIGRATION_HISTORY_TTL);
                assert_eq!(storage.get_ttl(&migration_count_key(env)), MIGRATION_HISTORY_TTL);
            }
            assert_eq!(current_version(env), SCHEMA_V2);
        });
    }

    // -----------------------------------------------------------------------
    // MigrationStep helpers
    // -----------------------------------------------------------------------

    #[test]
    fn test_step_to_v2_metadata() {
        let step = MigrationStep::ToV2;
        assert_eq!(step.required_from(), SCHEMA_V1);
        assert_eq!(step.produces(), SCHEMA_V2);
        assert_eq!(step.label(), "quotes_v1_to_v2");
    }
}
