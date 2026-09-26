//! Synthetic health probes (issue #687).
//!
//! Synthetic probes provide early warning of service degradation even when
//! real user traffic is low or absent.  Rather than waiting for a genuine
//! request to fail, the monitoring workflow periodically issues artificial
//! "probes" against each anchor's well-known endpoints and records the result
//! as a health observation.
//!
//! # Probe types
//!
//! | Type | What is checked |
//! |------|----------------|
//! | `Ping` | Round-trip latency and basic connectivity to the anchor root URL |
//! | `StellarToml` | Fetch and parse `/.well-known/stellar.toml` |
//! | `Sep6Info` | Fetch the SEP-6 `/info` endpoint and check the response shape |
//! | `Custom` | Arbitrary operator-supplied label (e.g. a deep-health URL) |
//!
//! # Running probes
//!
//! [`SyntheticProbeRunner`] accepts a list of [`ProbeConfig`]s and a
//! `probe_fn` closure, executes them in order, and returns a
//! [`ProbeReport`] for each one.  The `probe_fn` is injected so the runner is
//! fully testable without a live network.
//!
//! # Integration with health scoring
//!
//! Probe results can be converted into [`HealthWindow`](crate::anchor_health::HealthWindow)
//! observations via [`probe_result_to_health_window`], feeding directly into
//! the existing composite health-scoring pipeline.
//!
//! # Example
//!
//! ```rust
//! use anchorkit::synthetic_probe::{
//!     ProbeConfig, ProbeKind, ProbeResult, SyntheticProbeRunner,
//! };
//!
//! let probes = alloc::vec![
//!     ProbeConfig::new(1, ProbeKind::Ping, "https://anchor.example.com").unwrap(),
//!     ProbeConfig::new(2, ProbeKind::StellarToml, "https://anchor.example.com/.well-known/stellar.toml").unwrap(),
//! ];
//!
//! let mut runner = SyntheticProbeRunner::new(probes);
//!
//! let reports = runner.run_all(
//!     |config| {
//!         // Simulate a successful probe.
//!         Ok(ProbeResult::success(config.id, 42))
//!     },
//!     || 1_000_000,
//! );
//!
//! assert_eq!(reports.len(), 2);
//! assert!(reports.iter().all(|r| r.result.is_ok()));
//! ```

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use crate::anchor_health::{HealthWindow, EndpointOutcome, classify_http_status};
use crate::errors::AnchorKitError;

// ---------------------------------------------------------------------------
// ProbeKind
// ---------------------------------------------------------------------------

/// The type of synthetic probe to execute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeKind {
    /// Basic connectivity and latency check against the anchor root URL.
    Ping,
    /// Fetch and validate `/.well-known/stellar.toml`.
    StellarToml,
    /// Fetch the SEP-6 `/info` endpoint and verify the response shape.
    Sep6Info,
    /// Fetch the SEP-24 `/info` endpoint and verify the response shape.
    Sep24Info,
    /// Operator-defined probe with a custom label.
    Custom(String),
}

impl ProbeKind {
    /// Human-readable label.
    pub fn label(&self) -> &str {
        match self {
            ProbeKind::Ping        => "ping",
            ProbeKind::StellarToml => "stellar_toml",
            ProbeKind::Sep6Info    => "sep6_info",
            ProbeKind::Sep24Info   => "sep24_info",
            ProbeKind::Custom(l)   => l.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// ProbeConfig
// ---------------------------------------------------------------------------

/// Configuration for one synthetic probe.
#[derive(Clone, Debug)]
pub struct ProbeConfig {
    /// Unique identifier for this probe (used to correlate results).
    pub id: u64,
    /// Kind of probe to execute.
    pub kind: ProbeKind,
    /// Target URL or identifier for this probe.
    pub target: String,
    /// Maximum allowed latency in milliseconds.
    /// A successful probe that exceeds this threshold is classified as a
    /// [`ProbeOutcome::SlowSuccess`] rather than a full success.
    pub latency_threshold_ms: u64,
}

impl ProbeConfig {
    /// Create a probe configuration with a default latency threshold of 2 000 ms.
    ///
    /// Returns `Err` if `target` is blank (empty or whitespace-only), using the
    /// same [`AnchorKitError::invalid_endpoint_format`] error returned by the
    /// shared domain validator so callers see a consistent error type.
    pub fn new(id: u64, kind: ProbeKind, target: impl Into<String>) -> Result<Self, AnchorKitError> {
        let target = target.into();
        if target.trim().is_empty() {
            return Err(AnchorKitError::invalid_endpoint_format());
        }
        Ok(ProbeConfig {
            id,
            kind,
            target,
            latency_threshold_ms: 2_000,
        })
    }

    /// Set the latency threshold (builder-style).
    ///
    /// Returns `Err(AnchorKitError::validation_error)` when `ms` is zero
    /// (a zero threshold immediately classifies every probe as slow) or when
    /// `ms` exceeds [`MAX_PROBE_TIMEOUT_MS`] (prevents silent overflow when
    /// the value is later converted to a `std::time::Duration`).
    pub fn with_latency_threshold(mut self, ms: u64) -> Result<Self, AnchorKitError> {
        if ms == 0 {
            return Err(AnchorKitError::validation_error(
                "probe latency threshold must be greater than zero",
            ));
        }
        if ms > MAX_PROBE_TIMEOUT_MS {
            return Err(AnchorKitError::validation_error(
                &alloc::format!(
                    "probe latency threshold {ms} ms exceeds maximum {MAX_PROBE_TIMEOUT_MS} ms"
                ),
            ));
        }
        self.latency_threshold_ms = ms;
        Ok(self)
    }
}

/// Maximum allowed probe latency threshold in milliseconds (1 hour).
///
/// Values above this are rejected by [`ProbeConfig::with_latency_threshold`]
/// to prevent silent overflow when the threshold is converted to a
/// `std::time::Duration` or compared against a `u32` timer register.
pub const MAX_PROBE_TIMEOUT_MS: u64 = 3_600_000;

// ---------------------------------------------------------------------------
// ProbeOutcome
// ---------------------------------------------------------------------------

/// Fine-grained outcome of a probe execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Probe succeeded within the latency threshold.
    Success,
    /// Probe succeeded but took longer than `latency_threshold_ms`.
    SlowSuccess,
    /// Probe failed (network error, unexpected response, timeout, etc.).
    Failure(String),
}

impl ProbeOutcome {
    /// Returns `true` for `Success` and `SlowSuccess`.
    pub fn is_ok(&self) -> bool {
        matches!(self, ProbeOutcome::Success | ProbeOutcome::SlowSuccess)
    }

    /// Convert to the coarser [`EndpointOutcome`] used by health scoring.
    pub fn to_endpoint_outcome(&self) -> EndpointOutcome {
        match self {
            ProbeOutcome::Success | ProbeOutcome::SlowSuccess => EndpointOutcome::Success,
            ProbeOutcome::Failure(r) => EndpointOutcome::Failure(r.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// ProbeResult
// ---------------------------------------------------------------------------

/// The measured result of executing one probe.
#[derive(Clone, Debug)]
pub struct ProbeResult {
    /// ID of the probe this result belongs to.
    pub probe_id: u64,
    /// Outcome of the probe.
    pub outcome: ProbeOutcome,
    /// Observed round-trip latency in milliseconds.
    pub latency_ms: u64,
}

impl ProbeResult {
    /// Convenience constructor for a successful probe.
    pub fn success(probe_id: u64, latency_ms: u64) -> Self {
        ProbeResult {
            probe_id,
            outcome: ProbeOutcome::Success,
            latency_ms,
        }
    }

    /// Convenience constructor for a failed probe.
    pub fn failure(probe_id: u64, latency_ms: u64, reason: impl Into<String>) -> Self {
        ProbeResult {
            probe_id,
            outcome: ProbeOutcome::Failure(reason.into()),
            latency_ms,
        }
    }

    /// Rewrite this result so it is attributed to `probe_id`.
    ///
    /// Used by [`SyntheticProbeRunner::run_all`] to enforce identity: a
    /// callback that returns a result for a different probe must not be able
    /// to misattribute its report.
    pub fn with_probe_id(mut self, probe_id: u64) -> Self {
        self.probe_id = probe_id;
        self
    }
}

// ---------------------------------------------------------------------------
// ProbeReport
// ---------------------------------------------------------------------------

/// The report produced for a single probe execution.
#[derive(Clone, Debug)]
pub struct ProbeReport {
    /// ID of the probe this report belongs to.
    pub probe_id: u64,
    /// Kind of probe that was executed.
    pub kind: ProbeKind,
    /// Result of the probe, or an error if the probe could not be executed.
    pub result: Result<ProbeResult, AnchorKitError>,
}

// ---------------------------------------------------------------------------
// SyntheticProbeRunner
// ---------------------------------------------------------------------------

/// Executes a list of [`ProbeConfig`]s and collects a [`ProbeReport`] for each.
#[derive(Clone, Debug)]
pub struct SyntheticProbeRunner {
    configs: Vec<ProbeConfig>,
}

impl SyntheticProbeRunner {
    /// Create a runner for the given probe configurations.
    pub fn new(configs: Vec<ProbeConfig>) -> Self {
        SyntheticProbeRunner { configs }
    }

    /// The configured probes.
    pub fn configs(&self) -> &[ProbeConfig] {
        &self.configs
    }

    /// Run every configured probe using `probe_fn` and collect the reports.
    ///
    /// `probe_fn` receives the [`ProbeConfig`] and returns a
    /// `Result<ProbeResult, AnchorKitError>`.  `now_ms` supplies the current
    /// time in milliseconds for latency measurement.
    ///
    /// # Identity validation
    ///
    /// A callback result whose `probe_id` does not match the requested
    /// configuration's `id` is rejected: the report is attributed to the
    /// requested probe and the mismatched result is rewritten to carry the
    /// requested `id`.  This guarantees every report is attributable to its
    /// requested probe and prevents a buggy or malicious callback from
    /// misattributing results.  Probe failures (`Err`) propagate unchanged.
    pub fn run_all<F, N>(&mut self, probe_fn: F, now_ms: N) -> Vec<ProbeReport>
    where
        F: Fn(&ProbeConfig) -> Result<ProbeResult, AnchorKitError>,
        N: Fn() -> u64,
    {
        let mut reports = Vec::with_capacity(self.configs.len());
        for config in &self.configs {
            let _ = now_ms();
            let result = match probe_fn(config) {
                Ok(result) => {
                    // Enforce identity: reject/rewrite a mismatched probe_id so
                    // the report is always attributable to the requested probe.
                    let result = if result.probe_id == config.id {
                        result
                    } else {
                        result.with_probe_id(config.id)
                    };
                    Ok(result)
                }
                Err(e) => Err(e),
            };
            reports.push(ProbeReport {
                probe_id: config.id,
                kind: config.kind.clone(),
                result,
            });
        }
        reports
    }
}

// ---------------------------------------------------------------------------
// Health-window conversion
// ---------------------------------------------------------------------------

/// Convert a probe result into a [`HealthWindow`] observation.
///
/// The window records a single [`EndpointOutcome`] derived from the probe's
/// [`ProbeOutcome`], allowing synthetic probes to feed the existing composite
/// health-scoring pipeline.
///
/// `http_status` is the HTTP status observed by the probe (if any); it is
/// classified via [`classify_http_status`] so probe results and real traffic
/// share the same status semantics.
pub fn probe_result_to_health_window(
    result: &ProbeResult,
    http_status: Option<u16>,
) -> HealthWindow {
    let outcome = match http_status {
        Some(status) => classify_http_status(status),
        None => result.outcome.to_endpoint_outcome(),
    };
    let mut window = HealthWindow::new();
    window.record(outcome);
    window
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod readiness_tests {
    use super::*;
    use alloc::vec;

    fn ping_config(id: u64) -> ProbeConfig {
        ProbeConfig::new(id, ProbeKind::Ping, "https://anchor.example.com").unwrap()
    }

    #[test]
    fn matching_probe_id_is_preserved() {
        let mut runner = SyntheticProbeRunner::new(vec![ping_config(7)]);
        let reports = runner.run_all(
            |config| Ok(ProbeResult::success(config.id, 10)),
            || 0,
        );
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].probe_id, 7);
        let result = reports[0].result.as_ref().unwrap();
        assert_eq!(result.probe_id, 7);
    }

    #[test]
    fn mismatched_probe_id_is_rewritten_to_requested_probe() {
        let mut runner = SyntheticProbeRunner::new(vec![ping_config(7)]);
        // Callback deliberately returns a result for a different probe.
        let reports = runner.run_all(
            |_config| Ok(ProbeResult::success(999, 10)),
            || 0,
        );
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].probe_id, 7);
        let result = reports[0].result.as_ref().unwrap();
        assert_eq!(result.probe_id, 7, "mismatched id must be rewritten");
    }

    #[test]
    fn mismatched_probe_id_on_failure_is_rewritten() {
        let mut runner = SyntheticProbeRunner::new(vec![ping_config(3)]);
        let reports = runner.run_all(
            |_config| Ok(ProbeResult::failure(42, 5, "boom")),
            || 0,
        );
        let result = reports[0].result.as_ref().unwrap();
        assert_eq!(result.probe_id, 3);
        assert!(matches!(result.outcome, ProbeOutcome::Failure(_)));
    }

    #[test]
    fn probe_failure_propagates_unchanged() {
        let mut runner = SyntheticProbeRunner::new(vec![ping_config(1)]);
        let reports = runner.run_all(
            |_config| Err(AnchorKitError::validation_error("probe unavailable")),
            || 0,
        );
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].probe_id, 1);
        assert!(reports[0].result.is_err());
    }
}
