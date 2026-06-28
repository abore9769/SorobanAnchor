//! Streaming transaction monitor for long-running SEP-24 interactive flows.
//!
//! [`StreamingTransactionMonitor`] polls a transaction's state at a configurable
//! interval and emits [`TransactionStatusUpdate`] events for every state change.
//! It stops automatically when the transaction reaches a terminal state.

extern crate alloc;

use crate::retry::{retry_with_backoff, LedgerJitterSource, RetryConfig};
use crate::transaction_state_tracker::TransactionState;

// ── PollResult ────────────────────────────────────────────────────────────────

/// Return type for `poll_fn` passed to [`StreamingTransactionMonitor::run`].
///
/// Carries extra data (e.g. `stellar_tx_id`) that plain [`TransactionState`]
/// cannot represent.
#[derive(Clone, Debug, PartialEq)]
pub enum PollResult {
    /// Transaction is still in progress.
    Pending(TransactionState),
    /// Transaction completed; `stellar_tx_id` is the on-chain Stellar tx hash.
    Completed { stellar_tx_id: alloc::string::String },
    /// Transaction failed with a human-readable reason.
    Failed { reason: alloc::string::String },
}

// ── TransactionStatusUpdate ───────────────────────────────────────────────────

/// Events emitted by [`StreamingTransactionMonitor`] as a transaction progresses.
#[derive(Clone, Debug, PartialEq)]
pub enum TransactionStatusUpdate {
    /// The transaction moved from one state to another.
    StateChanged {
        from: TransactionState,
        to: TransactionState,
        timestamp: u64,
    },
    /// A more-info URL is available (e.g. SEP-24 interactive URL).
    MoreInfoAvailable { url: alloc::string::String },
    /// The transaction completed successfully.
    Completed { stellar_tx_id: alloc::string::String },
    /// The transaction failed.
    Failed { reason: alloc::string::String },
}

// ── StreamingTransactionMonitor ───────────────────────────────────────────────

/// Polls a transaction and emits [`TransactionStatusUpdate`] events on state changes.
///
/// # Example (pseudo-code — polling_fn is injected for testability)
///
/// ```rust,ignore
/// let mut monitor = StreamingTransactionMonitor::new(tx_id, 1000);
/// monitor.run(|id| fetch_state(id), |event| handle(event));
/// ```
pub struct StreamingTransactionMonitor {
    pub transaction_id: u64,
    /// Polling interval in milliseconds.
    pub poll_interval_ms: u64,
    retry_config: RetryConfig,
}

impl StreamingTransactionMonitor {
    pub fn new(transaction_id: u64, poll_interval_ms: u64) -> Self {
        Self {
            transaction_id,
            poll_interval_ms,
            retry_config: RetryConfig::default(),
        }
    }

    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Run the monitor.
    ///
    /// - `poll_fn`: given a transaction ID, returns `Ok(PollResult)` or `Err(String)`.
    /// - `on_event`: called for every [`TransactionStatusUpdate`] emitted.
    /// - `sleep_fn`: called with the poll interval (ms) between polls; inject `|_| {}` in tests.
    /// - `timestamp_fn`: called when emitting `StateChanged` events to obtain the current time.
    ///
    /// Returns when the transaction reaches a terminal state or all retries are exhausted.
    pub fn run<P, E, S, T>(
        &self,
        mut poll_fn: P,
        mut on_event: E,
        mut sleep_fn: S,
        timestamp_fn: T,
    ) where
        P: FnMut(u64) -> Result<PollResult, alloc::string::String>,
        E: FnMut(TransactionStatusUpdate),
        S: FnMut(u64),
        T: Fn() -> u64,
    {
        let mut last_state: Option<TransactionState> = None;
        let mut jitter = LedgerJitterSource::new(
            self.transaction_id as u32,
            timestamp_fn(),
        );

        loop {
            let result = retry_with_backoff(
                &self.retry_config,
                |_| poll_fn(self.transaction_id),
                |_| true, // all poll errors are retryable
                |ms| sleep_fn(ms),
                &mut jitter,
            );

            match result {
                Err(reason) => {
                    on_event(TransactionStatusUpdate::Failed { reason });
                    return;
                }
                Ok(PollResult::Failed { reason }) => {
                    on_event(TransactionStatusUpdate::Failed { reason });
                    return;
                }
                Ok(PollResult::Completed { stellar_tx_id }) => {
                    if let Some(prev) = last_state {
                        on_event(TransactionStatusUpdate::StateChanged {
                            from: prev,
                            to: TransactionState::Completed,
                            timestamp: timestamp_fn(),
                        });
                    }
                    on_event(TransactionStatusUpdate::Completed { stellar_tx_id });
                    return;
                }
                Ok(PollResult::Pending(current_state)) => {
                    if let Some(prev) = last_state {
                        if prev != current_state {
                            on_event(TransactionStatusUpdate::StateChanged {
                                from: prev,
                                to: current_state,
                                timestamp: timestamp_fn(),
                            });
                        }
                    }
                    last_state = Some(current_state);

                    if current_state == TransactionState::Failed {
                        on_event(TransactionStatusUpdate::Failed {
                            reason: alloc::string::String::from("transaction failed"),
                        });
                        return;
                    }
                }
            }

            sleep_fn(self.poll_interval_ms);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_state_tracker::TransactionState;

    #[test]
    fn test_monitor_emits_state_change_events() {
        let monitor = StreamingTransactionMonitor::new(1, 0);
        let states = alloc::vec![
            PollResult::Pending(TransactionState::Pending),
            PollResult::Pending(TransactionState::InProgress),
            PollResult::Completed { stellar_tx_id: alloc::string::String::from("abc") },
        ];
        let mut idx = 0usize;
        let mut events: alloc::vec::Vec<TransactionStatusUpdate> = alloc::vec::Vec::new();

        monitor.run(
            |_| {
                let s = states[idx.min(states.len() - 1)].clone();
                idx += 1;
                Ok(s)
            },
            |e| events.push(e),
            |_| {},
            || 1000,
        );

        assert!(events.iter().any(|e| matches!(e,
            TransactionStatusUpdate::StateChanged { from: TransactionState::Pending, to: TransactionState::InProgress, .. }
        )));
        assert!(events.iter().any(|e| matches!(e,
            TransactionStatusUpdate::StateChanged { from: TransactionState::InProgress, to: TransactionState::Completed, .. }
        )));
        assert!(events.iter().any(|e| matches!(e, TransactionStatusUpdate::Completed { stellar_tx_id } if stellar_tx_id == "abc")));
    }

    #[test]
    fn test_monitor_stops_on_completed() {
        let monitor = StreamingTransactionMonitor::new(1, 0);
        let mut call_count = 0u32;
        let mut events: alloc::vec::Vec<TransactionStatusUpdate> = alloc::vec::Vec::new();

        monitor.run(
            |_| {
                call_count += 1;
                Ok(PollResult::Completed { stellar_tx_id: alloc::string::String::from("tx1") })
            },
            |e| events.push(e),
            |_| {},
            || 0,
        );

        assert_eq!(call_count, 1);
        assert!(events.iter().any(|e| matches!(e, TransactionStatusUpdate::Completed { stellar_tx_id } if stellar_tx_id == "tx1")));
    }

    #[test]
    fn test_monitor_stops_on_failed() {
        let monitor = StreamingTransactionMonitor::new(1, 0);
        let mut events: alloc::vec::Vec<TransactionStatusUpdate> = alloc::vec::Vec::new();

        monitor.run(
            |_| Ok(PollResult::Pending(TransactionState::Failed)),
            |e| events.push(e),
            |_| {},
            || 0,
        );

        assert!(events.iter().any(|e| matches!(e, TransactionStatusUpdate::Failed { .. })));
    }

    #[test]
    fn test_monitor_retries_on_poll_failure() {
        let monitor = StreamingTransactionMonitor::new(1, 0);
        let mut call_count = 0u32;
        let mut events: alloc::vec::Vec<TransactionStatusUpdate> = alloc::vec::Vec::new();

        monitor.run(
            |_| {
                call_count += 1;
                if call_count < 3 {
                    Err(alloc::string::String::from("transient"))
                } else {
                    Ok(PollResult::Completed { stellar_tx_id: alloc::string::String::from("tx99") })
                }
            },
            |e| events.push(e),
            |_| {},
            || 0,
        );

        assert!(events.iter().any(|e| matches!(e, TransactionStatusUpdate::Completed { .. })));
    }

    #[test]
    fn test_monitor_emits_failed_when_all_retries_exhausted() {
        let monitor = StreamingTransactionMonitor::new(1, 0)
            .with_retry(RetryConfig::new(2, 0, 0, 1));
        let mut events: alloc::vec::Vec<TransactionStatusUpdate> = alloc::vec::Vec::new();

        monitor.run(
            |_| Err(alloc::string::String::from("permanent error")),
            |e| events.push(e),
            |_| {},
            || 0,
        );

        assert!(events.iter().any(|e| matches!(e, TransactionStatusUpdate::Failed { .. })));
    }

    #[test]
    fn test_poll_interval_is_configurable() {
        let monitor = StreamingTransactionMonitor::new(42, 500);
        assert_eq!(monitor.poll_interval_ms, 500);
    }

    #[test]
    fn test_state_changed_timestamp_uses_timestamp_fn() {
        let monitor = StreamingTransactionMonitor::new(1, 0);
        let mut events: alloc::vec::Vec<TransactionStatusUpdate> = alloc::vec::Vec::new();
        let states = alloc::vec![
            PollResult::Pending(TransactionState::Pending),
            PollResult::Pending(TransactionState::InProgress),
            PollResult::Completed { stellar_tx_id: alloc::string::String::new() },
        ];
        let mut idx = 0usize;

        monitor.run(
            |_| { let s = states[idx.min(states.len()-1)].clone(); idx += 1; Ok(s) },
            |e| events.push(e),
            |_| {},
            || 9999,
        );

        for e in &events {
            if let TransactionStatusUpdate::StateChanged { timestamp, .. } = e {
                assert_eq!(*timestamp, 9999);
            }
        }
    }

    #[test]
    fn test_completed_carries_stellar_tx_id() {
        let monitor = StreamingTransactionMonitor::new(1, 0);
        let mut events: alloc::vec::Vec<TransactionStatusUpdate> = alloc::vec::Vec::new();

        monitor.run(
            |_| Ok(PollResult::Completed { stellar_tx_id: alloc::string::String::from("HASH123") }),
            |e| events.push(e),
            |_| {},
            || 0,
        );

        assert!(events.iter().any(|e| matches!(e,
            TransactionStatusUpdate::Completed { stellar_tx_id } if stellar_tx_id == "HASH123"
        )));
    }
}
