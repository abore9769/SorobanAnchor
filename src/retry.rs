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

    /// Compute the delay (ms) for a given attempt index (0-based), with jitter.
    ///
    /// delay = min(base * multiplier^attempt, max) + jitter(0..base/2)
    pub fn delay_for_attempt(&self, attempt: u32, jitter_seed: u64) -> u64 {
        let exp = (self.backoff_multiplier as u64).saturating_pow(attempt);
        let raw = self.base_delay_ms.saturating_mul(exp);
        let capped = raw.min(self.max_delay_ms);
        // Simple deterministic jitter: seed % (base_delay_ms / 2 + 1)
        let jitter = jitter_seed % (self.base_delay_ms / 2 + 1);
        capped.saturating_add(jitter)
    }
}

/// Classify whether an error code is retryable.
///
/// Retryable: transient network/server errors.
/// Non-retryable: auth failures, bad input, protocol violations.
pub fn is_retryable(code: u32) -> bool {
    use crate::errors::ErrorCode;
    matches!(
        code,
        // transport / availability
        _ if code == ErrorCode::ServicesNotConfigured as u32
            || code == ErrorCode::AttestationNotFound as u32
            || code == ErrorCode::StaleQuote as u32
            || code == ErrorCode::NoQuotesAvailable as u32
            || code == ErrorCode::CacheExpired as u32
            || code == ErrorCode::CacheNotFound as u32
            || code == ErrorCode::RateLimitExceeded as u32
    )
}

/// Execute `f` with exponential backoff retry.
///
/// `f` receives the current attempt number (0-based) and returns `Ok(T)` on
/// success or `Err(E)` on failure.  `retryable` classifies whether an error
/// warrants another attempt.
///
/// A `sleep_fn` callback is provided so callers can inject real or mock sleep
/// (avoids pulling in `std::thread::sleep` or async runtimes).
pub fn retry_with_backoff<T, E, F, S>(
    config: &RetryConfig,
    mut f: F,
    retryable: impl Fn(&E) -> bool,
    mut sleep_fn: S,
) -> Result<T, E>
where
    F: FnMut(u32) -> Result<T, E>,
    S: FnMut(u64),
{
    let mut last_err: Option<E> = None;

    for attempt in 0..config.max_attempts {
        match f(attempt) {
            Ok(val) => return Ok(val),
            Err(e) => {
                if !retryable(&e) || attempt + 1 >= config.max_attempts {
                    return Err(e);
                }
                let delay = config.delay_for_attempt(attempt, attempt as u64 * 17 + 3);
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
        let result = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Ok::<_, TestError>(42)
            },
            is_retryable_test,
            |_| {},
        );
        assert_eq!(result, Ok(42));
        assert_eq!(calls, 1);
    }

    #[test]
    fn test_success_after_retry() {
        let config = RetryConfig::default();
        let mut calls = 0u32;
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
        );
        assert_eq!(result, Ok(99));
        assert_eq!(calls, 3);
    }

    #[test]
    fn test_exhausted_retries() {
        let config = RetryConfig::new(3, 10, 1000, 2);
        let mut calls = 0u32;
        let result = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Transient)
            },
            is_retryable_test,
            |_| {},
        );
        assert_eq!(result, Err(TestError::Transient));
        assert_eq!(calls, 3);
    }

    #[test]
    fn test_non_retryable_error_stops_immediately() {
        let config = RetryConfig::new(5, 10, 1000, 2);
        let mut calls = 0u32;
        let result = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Permanent)
            },
            is_retryable_test,
            |_| {},
        );
        assert_eq!(result, Err(TestError::Permanent));
        assert_eq!(calls, 1);
    }

    #[test]
    fn test_delay_increases_exponentially() {
        let config = RetryConfig::new(4, 100, 10_000, 2);
        // attempt 0: 100 * 2^0 = 100, attempt 1: 200, attempt 2: 400
        assert!(config.delay_for_attempt(0, 0) >= 100);
        assert!(config.delay_for_attempt(1, 0) >= 200);
        assert!(config.delay_for_attempt(2, 0) >= 400);
    }

    #[test]
    fn test_delay_capped_at_max() {
        let config = RetryConfig::new(10, 1000, 3_000, 2);
        // attempt 5: 1000 * 2^5 = 32000, capped at 3000
        assert!(config.delay_for_attempt(5, 0) <= 3_000 + config.base_delay_ms / 2 + 1);
    }

    #[test]
    fn test_sleep_called_between_retries() {
        let config = RetryConfig::new(3, 50, 5000, 2);
        let mut sleep_calls = 0u32;
        let _ = retry_with_backoff(
            &config,
            |_| Err::<i32, _>(TestError::Transient),
            is_retryable_test,
            |_| sleep_calls += 1,
        );
        // 3 attempts → 2 sleeps (no sleep after last attempt)
        assert_eq!(sleep_calls, 2);
    }

    /// Verify that the delay values passed to sleep_fn match the expected
    /// exponential sequence.  Uses a mock clock (a Vec accumulator) instead of
    /// a no-op so we can inspect every value.
    #[test]
    fn test_mock_clock_delay_sequence() {
        // 4 attempts, 100 ms base, 10 s cap, multiplier 2 → delays for
        // attempts 0, 1, 2 (no sleep after the last attempt):
        //   attempt 0: 100 * 2^0 = 100  + jitter(seed=3  % 51) = 100 + 3  = 103
        //   attempt 1: 100 * 2^1 = 200  + jitter(seed=20 % 51) = 200 + 20 = 220
        //   attempt 2: 100 * 2^2 = 400  + jitter(seed=37 % 51) = 400 + 37 = 437
        // (jitter_seed = attempt * 17 + 3, jitter = seed % (base/2 + 1) = seed % 51)
        let config = RetryConfig::new(4, 100, 10_000, 2);
        let mut recorded: Vec<u64> = Vec::new();

        let _ = retry_with_backoff(
            &config,
            |_| Err::<i32, _>(TestError::Transient),
            is_retryable_test,
            |ms| recorded.push(ms),
        );

        // Three sleeps for four attempts.
        assert_eq!(recorded.len(), 3, "expected 3 sleeps for 4 attempts");

        // Each recorded delay must be >= the pure exponential value (jitter only adds).
        let base = config.base_delay_ms;
        let mult = config.backoff_multiplier as u64;
        for (i, &actual) in recorded.iter().enumerate() {
            let expected_min = base.saturating_mul(mult.saturating_pow(i as u32));
            assert!(
                actual >= expected_min,
                "attempt {i}: delay {actual} < expected minimum {expected_min}"
            );
        }

        // Each delay must be strictly greater than the previous (exponential growth).
        for window in recorded.windows(2) {
            assert!(
                window[1] > window[0],
                "delays are not strictly increasing: {:?}",
                recorded
            );
        }

        // Verify exact values match the deterministic jitter formula used in
        // retry_with_backoff (jitter_seed = attempt * 17 + 3).
        let expected: Vec<u64> = (0..3u32)
            .map(|a| config.delay_for_attempt(a, a as u64 * 17 + 3))
            .collect();
        assert_eq!(recorded, expected, "recorded delays don't match expected sequence");
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
        let _ = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Transient)
            },
            is_retryable_test,
            |_| {},
        );
        assert_eq!(calls, 5);
    }

    #[test]
    fn test_conservative_stops_after_two_attempts() {
        let config = RetryConfig::conservative();
        let mut calls = 0u32;
        let _ = retry_with_backoff(
            &config,
            |_| {
                calls += 1;
                Err::<i32, _>(TestError::Transient)
            },
            is_retryable_test,
            |_| {},
        );
        assert_eq!(calls, 2);
    }
}
