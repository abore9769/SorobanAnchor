/// Retry configuration for off-chain anchor requests.
///
/// Controls how many times a failing operation is retried and how long to wait
/// between attempts. The delay grows exponentially and is capped at
/// `max_delay_ms` to prevent unbounded waits.
///
/// # Examples
///
/// ```rust
/// use anchorkit::RetryConfig;
///
/// // Use sensible defaults: 3 attempts, 100 ms base, 5 s cap, ×2 multiplier.
/// let config = RetryConfig::default();
/// assert_eq!(config.max_attempts, 3);
///
/// // Custom configuration for a high-latency anchor.
/// let config = RetryConfig::new(5, 200, 10_000, 3);
/// assert_eq!(config.max_attempts, 5);
/// ```
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
    /// Create a [`RetryConfig`] with explicit values for all fields.
    ///
    /// # Arguments
    ///
    /// * `max_attempts` - Total number of attempts including the first try.
    ///   Must be at least `1`.
    /// * `base_delay_ms` - Delay in milliseconds before the first retry.
    /// * `max_delay_ms` - Upper bound on the computed delay (caps exponential growth).
    /// * `backoff_multiplier` - Factor by which the delay is multiplied each attempt.
    ///
    /// # Returns
    ///
    /// A new [`RetryConfig`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use anchorkit::RetryConfig;
    ///
    /// let config = RetryConfig::new(5, 200, 10_000, 3);
    /// assert_eq!(config.max_attempts, 5);
    /// assert_eq!(config.base_delay_ms, 200);
    /// ```
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

    /// Compute the delay (ms) for a given attempt index (0-based), with jitter.
    ///
    /// The formula is:
    /// `delay = min(base_delay_ms × backoff_multiplier^attempt, max_delay_ms) + jitter`
    ///
    /// where `jitter = jitter_seed % (base_delay_ms / 2 + 1)`.
    ///
    /// # Arguments
    ///
    /// * `attempt` - Zero-based attempt index (0 = first retry).
    /// * `jitter_seed` - Deterministic seed used to compute a small jitter offset.
    ///
    /// # Returns
    ///
    /// Delay in milliseconds for this attempt.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use anchorkit::RetryConfig;
    ///
    /// let config = RetryConfig::new(4, 100, 10_000, 2);
    /// assert!(config.delay_for_attempt(0, 0) >= 100);
    /// assert!(config.delay_for_attempt(1, 0) >= 200);
    /// assert!(config.delay_for_attempt(2, 0) >= 400);
    /// ```
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
/// Returns `true` for transient conditions where retrying may succeed (e.g.
/// rate limiting, stale cache, unavailable quotes). Returns `false` for
/// permanent failures such as auth errors or bad input.
///
/// # Arguments
///
/// * `code` - The raw `u32` discriminant of an [`ErrorCode`] variant.
///
/// # Returns
///
/// `true` if the error is transient and the operation should be retried.
///
/// # Examples
///
/// ```rust
/// use anchorkit::{retry::is_retryable, ErrorCode};
///
/// assert!(is_retryable(ErrorCode::RateLimitExceeded as u32));
/// assert!(is_retryable(ErrorCode::StaleQuote as u32));
/// assert!(!is_retryable(ErrorCode::ReplayAttack as u32));
/// assert!(!is_retryable(ErrorCode::AlreadyInitialized as u32));
/// ```
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
/// Calls `f` up to `config.max_attempts` times. After each failure that
/// `retryable` classifies as transient, waits for the computed backoff delay
/// (via `sleep_fn`) before trying again. Stops immediately on a non-retryable
/// error or when all attempts are exhausted.
///
/// # Arguments
///
/// * `config` - Retry parameters (attempts, delays, multiplier).
/// * `f` - The fallible operation. Receives the current attempt index (0-based).
/// * `retryable` - Predicate that returns `true` when an error warrants a retry.
/// * `sleep_fn` - Callback invoked with the delay in milliseconds between attempts.
///   Inject `|_| {}` in tests to avoid real sleeps.
///
/// # Returns
///
/// `Ok(T)` on the first successful attempt, or `Err(E)` after all attempts are
/// exhausted or a non-retryable error is encountered.
///
/// # Errors
///
/// Returns the last error produced by `f`. The error is non-retryable if
/// `retryable` returned `false`, or all `max_attempts` were consumed.
///
/// # Examples
///
/// ```rust
/// use anchorkit::retry::{retry_with_backoff, RetryConfig};
///
/// let config = RetryConfig::default();
/// let mut calls = 0u32;
///
/// let result = retry_with_backoff(
///     &config,
///     |attempt| {
///         calls += 1;
///         if attempt < 2 { Err("transient") } else { Ok(42u32) }
///     },
///     |_err| true,   // all errors are retryable
///     |_ms| {},      // no-op sleep
/// );
///
/// assert_eq!(result, Ok(42));
/// assert_eq!(calls, 3);
/// ```
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
}
