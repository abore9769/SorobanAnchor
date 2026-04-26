use Vec;

/// Retry configuration for off-chain anchor requests.
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first try).
    pub max_attempts: u32,
    /// Initial delay in milliseconds before the first retry.
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds (caps exponential growth).
    pub max_delay_ms: u64,
    /// Multiplier applied to the delay after each failed attempt.
    pub backoff_multiplier: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_attempts: 3,
            base_delay_ms: 100,
            max_delay_ms: 5_000,
            backoff_multiplier: 2,
        }
    }
}

impl RetryConfig {
    pub fn new(
        max_attempts: u32,
        base_delay_ms: u64,
        max_delay_ms: u64,
        backoff_multiplier: u32,
    ) -> Self {
        RetryConfig {
            max_attempts,
            base_delay_ms,
            max_delay_ms,
            backoff_multiplier,
        }
    }

    /// 5 attempts, 50 ms base, 2 s max — for time-sensitive operations.
    pub fn aggressive() -> Self {
        RetryConfig {
            max_attempts: 5,
            base_delay_ms: 50,
            max_delay_ms: 2_000,
            backoff_multiplier: 2,
        }
    }

    /// 2 attempts, 500 ms base, 10 s max — for conservative/low-noise retries.
    pub fn conservative() -> Self {
        RetryConfig {
            max_attempts: 2,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
            backoff_multiplier: 2,
        }
    }

    /// Compute the delay (ms) for a given attempt index (0-based), drawing
    /// jitter from `jitter_source`.
    ///
    /// delay = min(base * multiplier^attempt, max) + jitter(0..base/2)
    pub fn delay_for_attempt(&self, attempt: u32, jitter_source: &mut impl JitterSource) -> u64 {
        let exp = (self.backoff_multiplier as u64).saturating_pow(attempt);
        let raw = self.base_delay_ms.saturating_mul(exp);
        let capped = raw.min(self.max_delay_ms);
        let jitter = jitter_source.next_seed() % (self.base_delay_ms / 2 + 1);
        capped.saturating_add(jitter)
    }
}

// ---------------------------------------------------------------------------
// JitterSource trait
// ---------------------------------------------------------------------------

/// Provides a seed value for jitter computation on each retry attempt.
///
/// Implementations must produce values that differ across consecutive calls
/// to avoid the thundering-herd problem when multiple clients retry together.
pub trait JitterSource {
    fn next_seed(&mut self) -> u64;
}

// ---------------------------------------------------------------------------
// LedgerJitterSource
// ---------------------------------------------------------------------------

/// Derives jitter seeds from Soroban ledger state.
///
/// XORs `sequence ^ timestamp ^ counter` so that consecutive calls within
/// the same ledger still produce different seeds.
pub struct LedgerJitterSource {
    sequence: u32,
    timestamp: u64,
    counter: u64,
}

impl LedgerJitterSource {
    pub fn new(sequence: u32, timestamp: u64) -> Self {
        LedgerJitterSource { sequence, timestamp, counter: 0 }
    }
}

impl JitterSource for LedgerJitterSource {
    fn next_seed(&mut self) -> u64 {
        let seed = (self.sequence as u64) ^ self.timestamp ^ self.counter;
        self.counter = self.counter.wrapping_add(1);
        seed
    }
}

// ---------------------------------------------------------------------------
// MockJitterSource
// ---------------------------------------------------------------------------

/// Produces a pre-configured sequence of seeds for deterministic testing.
/// Cycles back to the start when the sequence is exhausted.
pub struct MockJitterSource {
    seeds: Vec<u64>,
    index: usize,
}

impl MockJitterSource {
    pub fn new(seeds: Vec<u64>) -> Self {
        MockJitterSource { seeds, index: 0 }
    }
}

impl JitterSource for MockJitterSource {
    fn next_seed(&mut self) -> u64 {
        if self.seeds.is_empty() {
            return 0;
        }
        let seed = self.seeds[self.index % self.seeds.len()];
        self.index += 1;
        seed
    }
}

// ---------------------------------------------------------------------------
// Classify whether an error code is retryable.
// ---------------------------------------------------------------------------

/// Classify whether an error code is retryable.
///
/// Retryable: transient network/server errors (availability, rate limits, stale data).
/// Non-retryable: auth failures, bad input, protocol violations.
pub fn is_retryable(code: crate::errors::ErrorCode) -> bool {
    use crate::errors::ErrorCode;
    match code {
        ErrorCode::ServicesNotConfigured
        | ErrorCode::AttestationNotFound
        | ErrorCode::StaleQuote
        | ErrorCode::NoQuotesAvailable
        | ErrorCode::CacheExpired
        | ErrorCode::CacheNotFound
        | ErrorCode::RateLimitExceeded => true,
        _ => false,
    }
}

/// Execute `f` with exponential backoff retry.
///
/// `f` receives the current attempt number (0-based) and returns `Ok(T)` on
/// success or `Err(E)` on failure.  `retryable` classifies whether an error
/// warrants another attempt.
///
/// A `sleep_fn` callback is provided so callers can inject real or mock sleep.
/// `jitter_source` provides per-attempt seeds to spread retry timing.
pub fn retry_with_backoff<T, E, F, S, J>(
    config: &RetryConfig,
    mut f: F,
    retryable: impl Fn(&E) -> bool,
    mut sleep_fn: S,
    jitter_source: &mut J,
) -> Result<T, E>
where
    F: FnMut(u32) -> Result<T, E>,
    S: FnMut(u64),
    J: JitterSource,
{
    let mut last_err: Option<E> = None;

    for attempt in 0..config.max_attempts {
        match f(attempt) {
            Ok(val) => return Ok(val),
            Err(e) => {
                if !retryable(&e) || attempt + 1 >= config.max_attempts {
                    return Err(e);
                }
                let delay = config.delay_for_attempt(attempt, jitter_source);
                sleep_fn(delay);
                last_err = Some(e);
            }
        }
    }

    Err(last_err.expect("max_attempts must be >= 1"))
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum TestError {
        Transient,
        Permanent,
    }

    fn is_retryable_test(e: &TestError) -> bool {
        matches!(e, TestError::Transient)
    }

    #[test]
    fn test_success_on_first_try() {
        let config = RetryConfig::default();
        let mut calls = 0u32;
        let mut js = MockJitterSource::new(vec![0]);
        let result = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Ok::<_, TestError>(42)
            },
            is_retryable_test,
            |_| {},
            &mut js,
        );
        assert_eq!(result, Ok(42));
        assert_eq!(calls, 1);
    }

    #[test]
    fn test_success_after_retry() {
        let config = RetryConfig::default();
        let mut calls = 0u32;
        let mut js = MockJitterSource::new(vec![0, 0, 0]);
        let result = retry_with_backoff(
            &config,
            |attempt| {
                calls += 1;
                if attempt < 2 {
                    Err(TestError::Transient)
                } else {
                    Ok(99)
                }
            },
            is_retryable_test,
            |_| {},
            &mut js,
        );
        assert_eq!(result, Ok(99));
        assert_eq!(calls, 3);
    }

    #[test]
    fn test_exhausted_retries() {
        let config = RetryConfig::new(3, 10, 1000, 2);
        let mut calls = 0u32;
        let mut js = MockJitterSource::new(vec![0]);
        let result = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Transient)
            },
            is_retryable_test,
            |_| {},
            &mut js,
        );
        assert_eq!(result, Err(TestError::Transient));
        assert_eq!(calls, 3);
    }

    #[test]
    fn test_non_retryable_error_stops_immediately() {
        let config = RetryConfig::new(5, 10, 1000, 2);
        let mut calls = 0u32;
        let mut js = MockJitterSource::new(vec![0]);
        let result = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Permanent)
            },
            is_retryable_test,
            |_| {},
            &mut js,
        );
        assert_eq!(result, Err(TestError::Permanent));
        assert_eq!(calls, 1);
    }

    #[test]
    fn test_delay_increases_exponentially() {
        let config = RetryConfig::new(4, 100, 10_000, 2);
        let mut js = MockJitterSource::new(vec![0]);
        assert!(config.delay_for_attempt(0, &mut js) >= 100);
        assert!(config.delay_for_attempt(1, &mut js) >= 200);
        assert!(config.delay_for_attempt(2, &mut js) >= 400);
    }

    #[test]
    fn test_delay_capped_at_max() {
        let config = RetryConfig::new(10, 1000, 3_000, 2);
        let mut js = MockJitterSource::new(vec![0]);
        assert!(config.delay_for_attempt(5, &mut js) <= 3_000 + config.base_delay_ms / 2 + 1);
    }

    #[test]
    fn test_sleep_called_between_retries() {
        let config = RetryConfig::new(3, 50, 5000, 2);
        let mut sleep_calls = 0u32;
        let mut js = MockJitterSource::new(vec![0]);
        let _ = retry_with_backoff(
            &config,
            |_| Err::<i32, _>(TestError::Transient),
            is_retryable_test,
            |_| sleep_calls += 1,
            &mut js,
        );
        assert_eq!(sleep_calls, 2);
    }

    #[test]
    fn test_aggressive_config() {
        let cfg = RetryConfig::aggressive();
        assert_eq!(cfg.max_attempts, 5);
        assert_eq!(cfg.base_delay_ms, 50);
        assert_eq!(cfg.max_delay_ms, 2_000);
        assert_eq!(cfg.backoff_multiplier, 2);
    }

    #[test]
    fn test_conservative_config() {
        let cfg = RetryConfig::conservative();
        assert_eq!(cfg.max_attempts, 2);
        assert_eq!(cfg.base_delay_ms, 500);
        assert_eq!(cfg.max_delay_ms, 10_000);
        assert_eq!(cfg.backoff_multiplier, 2);
    }

    #[test]
    fn test_aggressive_retries_up_to_five_attempts() {
        let config = RetryConfig::aggressive();
        let mut calls = 0u32;
        let mut js = MockJitterSource::new(vec![0]);
        let _ = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Transient)
            },
            is_retryable_test,
            |_| {},
            &mut js,
        );
        assert_eq!(calls, 5);
    }

    #[test]
    fn test_conservative_stops_after_two_attempts() {
        let config = RetryConfig::conservative();
        let mut calls = 0u32;
        let mut js = MockJitterSource::new(vec![0]);
        let _ = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Transient)
            },
            is_retryable_test,
            |_| {},
            &mut js,
        );
        assert_eq!(calls, 2);
    }

    // -----------------------------------------------------------------------
    // New tests for JitterSource
    // -----------------------------------------------------------------------

    /// Two retries with different seeds produce different delays.
    #[test]
    fn test_different_seeds_produce_different_delays() {
        let config = RetryConfig::new(4, 100, 10_000, 2);
        let mut js_a = MockJitterSource::new(vec![0]);
        let mut js_b = MockJitterSource::new(vec![49]); // max jitter for base=100
        let delay_a = config.delay_for_attempt(0, &mut js_a);
        let delay_b = config.delay_for_attempt(0, &mut js_b);
        assert_ne!(delay_a, delay_b);
    }

    /// Delay is always within configured bounds (base..=max + max_jitter).
    #[test]
    fn test_delay_within_bounds() {
        let config = RetryConfig::new(6, 100, 3_000, 2);
        let max_jitter = config.base_delay_ms / 2; // 50
        for seed in [0u64, 1, 25, 49, 50, 99, 1000] {
            for attempt in 0..6u32 {
                let mut js = MockJitterSource::new(vec![seed]);
                let delay = config.delay_for_attempt(attempt, &mut js);
                assert!(delay >= config.base_delay_ms, "delay {delay} < base");
                assert!(
                    delay <= config.max_delay_ms + max_jitter,
                    "delay {delay} > max+jitter"
                );
            }
        }
    }

    /// MockJitterSource produces deterministic results in the specified order.
    #[test]
    fn test_mock_source_deterministic() {
        let config = RetryConfig::new(4, 100, 10_000, 2);
        let seeds = vec![10u64, 20, 30];
        let mut js = MockJitterSource::new(seeds.clone());

        let d0 = config.delay_for_attempt(0, &mut js); // seed=10, jitter=10%51=10
        let d1 = config.delay_for_attempt(1, &mut js); // seed=20, jitter=20%51=20
        let d2 = config.delay_for_attempt(2, &mut js); // seed=30, jitter=30%51=30

        assert_eq!(d0, 100 + 10); // 100 * 2^0 + 10
        assert_eq!(d1, 200 + 20); // 100 * 2^1 + 20
        assert_eq!(d2, 400 + 30); // 100 * 2^2 + 30
    }

    /// LedgerJitterSource produces different seeds on consecutive calls.
    #[test]
    fn test_ledger_jitter_source_consecutive_differ() {
        let mut js = LedgerJitterSource::new(42, 1_000_000);
        let s0 = js.next_seed();
        let s1 = js.next_seed();
        let s2 = js.next_seed();
        assert_ne!(s0, s1);
        assert_ne!(s1, s2);
    }

    /// retry_with_backoff passes jitter_source through to delay_for_attempt.
    #[test]
    fn test_mock_clock_delay_sequence() {
        let config = RetryConfig::new(4, 100, 10_000, 2);
        // seeds: 3, 20, 37 → jitter: 3%51=3, 20%51=20, 37%51=37
        let mut js = MockJitterSource::new(vec![3, 20, 37]);
        let mut recorded: Vec<u64> = Vec::new();

        let _ = retry_with_backoff(
            &config,
            |_| Err::<i32, _>(TestError::Transient),
            is_retryable_test,
            |ms| recorded.push(ms),
            &mut js,
        );

        assert_eq!(recorded.len(), 3);
        assert_eq!(recorded[0], 100 + 3);  // attempt 0: 100*2^0 + 3
        assert_eq!(recorded[1], 200 + 20); // attempt 1: 100*2^1 + 20
        assert_eq!(recorded[2], 400 + 37); // attempt 2: 100*2^2 + 37
    }
}
