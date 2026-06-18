//! Rate limiting for library write operations.
//!
//! This module prevents runaway AI operations by limiting the rate of
//! destructive/mutating tool calls (writes, deletes, renames, batch updates).
//! Because every overwrite first copies the whole OLE file to a timestamped
//! `.bak`, an unbounded loop would thrash disk and I/O with repeated full-file
//! rewrites; the limiter caps that.
//!
//! # Algorithm
//!
//! Uses a token bucket algorithm:
//! - Bucket starts with `max_burst` tokens
//! - Each operation consumes one token
//! - Tokens are replenished at `refill_rate` per second
//! - If no tokens available, operation is blocked
//!
//! # Mutex Poisoning
//!
//! This module handles mutex poisoning gracefully. If a thread panics while
//! holding a lock, subsequent operations will recover by extracting the inner
//! value from the poisoned mutex. For a rate limiter, having potentially stale
//! state is preferable to crashing the entire application.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Rate limiter using token bucket algorithm.
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum tokens in the bucket (burst capacity).
    max_burst: u64,

    /// Current tokens available.
    tokens: Mutex<f64>,

    /// Tokens added per second.
    refill_rate: f64,

    /// Last time tokens were refilled.
    last_refill: Mutex<Instant>,

    /// Total operations allowed (lifetime).
    total_allowed: AtomicU64,

    /// Total operations blocked (lifetime).
    total_blocked: AtomicU64,
}

impl RateLimiter {
    /// Creates a new rate limiter.
    ///
    /// # Arguments
    ///
    /// * `max_burst` — Maximum operations allowed in a burst
    /// * `refill_rate` — Operations allowed per second (sustained rate)
    ///
    /// # Example
    ///
    /// ```
    /// use altium_designer_mcp::security::RateLimiter;
    ///
    /// // Allow burst of 10 operations, sustained rate of 2/second
    /// let limiter = RateLimiter::new(10, 2.0);
    /// ```
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // max_burst as f64 is acceptable
    pub fn new(max_burst: u64, refill_rate: f64) -> Self {
        Self {
            max_burst,
            tokens: Mutex::new(max_burst as f64),
            refill_rate,
            last_refill: Mutex::new(Instant::now()),
            total_allowed: AtomicU64::new(0),
            total_blocked: AtomicU64::new(0),
        }
    }

    /// Creates a rate limiter with sensible defaults for local write
    /// operations.
    ///
    /// Default: 120 burst, 30 operations/second. Higher than a network tool
    /// would use, since local OLE file I/O is comparatively cheap — the goal is
    /// only to stop an unbounded loop, not to throttle legitimate batch work.
    #[must_use]
    pub fn default_for_writes() -> Self {
        Self::new(120, 30.0)
    }

    /// Creates a rate limiter that allows unlimited operations.
    ///
    /// Used as the default for tests and for the bare [`crate::mcp`] server
    /// constructor; production installs a configured limiter.
    #[must_use]
    pub fn unlimited() -> Self {
        Self::new(u64::MAX, f64::MAX)
    }

    /// Locks the tokens mutex, recovering from poison if necessary.
    ///
    /// If the mutex is poisoned (a thread panicked while holding the lock),
    /// this method recovers by extracting the inner value. A warning is logged
    /// when recovery occurs.
    fn lock_tokens(&self) -> MutexGuard<'_, f64> {
        self.tokens.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "Rate limiter tokens mutex was poisoned; recovering with potentially stale state"
            );
            poisoned.into_inner()
        })
    }

    /// Locks the `last_refill` mutex, recovering from poison if necessary.
    fn lock_last_refill(&self) -> MutexGuard<'_, Instant> {
        self.last_refill.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "Rate limiter last_refill mutex was poisoned; recovering with potentially stale state"
            );
            poisoned.into_inner()
        })
    }

    /// Attempts to acquire a token for an operation.
    ///
    /// Returns `true` if the operation is allowed, `false` if rate limited.
    #[allow(clippy::significant_drop_tightening)] // Lock must be held during update
    pub fn try_acquire(&self) -> bool {
        self.refill();

        let mut tokens = self.lock_tokens();

        if *tokens >= 1.0 {
            *tokens -= 1.0;
            drop(tokens);
            self.total_allowed.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            drop(tokens);
            self.total_blocked.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    /// Checks if an operation would be allowed without consuming a token.
    #[must_use]
    pub fn would_allow(&self) -> bool {
        self.refill();
        let tokens = self.lock_tokens();
        *tokens >= 1.0
    }

    /// Returns the current number of available tokens.
    #[must_use]
    pub fn available_tokens(&self) -> f64 {
        self.refill();
        let tokens = self.lock_tokens();
        *tokens
    }

    /// Returns time until the next token is available.
    ///
    /// Returns `Duration::ZERO` if tokens are currently available, and
    /// `Duration::MAX` ("effectively never") when the bucket cannot refill — a
    /// `refill_rate` of zero or less, or a rate so small that the computed wait
    /// would overflow `Duration`.
    #[must_use]
    pub fn time_until_available(&self) -> Duration {
        self.refill();

        let tokens = self.lock_tokens();
        let current_tokens = *tokens;
        drop(tokens);

        if current_tokens >= 1.0 {
            Duration::ZERO
        } else if self.refill_rate <= 0.0 {
            // No refill rate means tokens will never be available
            Duration::MAX
        } else {
            let tokens_needed = 1.0 - current_tokens;
            let seconds = tokens_needed / self.refill_rate;
            // A minuscule (but config-valid: finite and positive) refill_rate
            // makes `seconds` exceed Duration's range, and `from_secs_f64`
            // panics on overflow/non-finite input. Saturate to `Duration::MAX`
            // — semantically "effectively never" — instead of crashing.
            Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX)
        }
    }

    /// Refills tokens based on elapsed time.
    #[allow(clippy::significant_drop_tightening)] // Lock ordering is intentional
    #[allow(clippy::cast_precision_loss)] // max_burst as f64 is acceptable
    fn refill(&self) {
        // A non-positive refill rate means the bucket never refills: 0.0 is the
        // supported "burst once" mode, and a negative rate (rejected by config
        // but accepted by the public constructor) would otherwise *drain*
        // tokens, since `elapsed * negative` is negative. Skip entirely — this
        // matches `time_until_available`'s "rate <= 0 means never available"
        // contract and avoids locking the tokens mutex in the 0.0 mode.
        if self.refill_rate <= 0.0 {
            return;
        }

        let now = Instant::now();

        let mut last_refill = self.lock_last_refill();
        let elapsed = now.duration_since(*last_refill);

        if elapsed.as_secs_f64() > 0.0 {
            let mut tokens = self.lock_tokens();

            let new_tokens = elapsed.as_secs_f64() * self.refill_rate;
            *tokens = (*tokens + new_tokens).min(self.max_burst as f64);

            *last_refill = now;
        }
    }

    /// Returns statistics about rate limiting.
    #[must_use]
    pub fn stats(&self) -> RateLimiterStats {
        RateLimiterStats {
            total_allowed: self.total_allowed.load(Ordering::Relaxed),
            total_blocked: self.total_blocked.load(Ordering::Relaxed),
            available_tokens: self.available_tokens(),
            max_burst: self.max_burst,
            refill_rate: self.refill_rate,
        }
    }

    /// Resets the rate limiter to its initial state.
    #[allow(clippy::significant_drop_tightening)] // Locks are independent
    #[allow(clippy::cast_precision_loss)] // max_burst as f64 is acceptable
    pub fn reset(&self) {
        let mut tokens = self.lock_tokens();
        *tokens = self.max_burst as f64;
        drop(tokens);

        let mut last_refill = self.lock_last_refill();
        *last_refill = Instant::now();
        drop(last_refill);

        self.total_allowed.store(0, Ordering::Relaxed);
        self.total_blocked.store(0, Ordering::Relaxed);
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::default_for_writes()
    }
}

/// Statistics about rate limiter usage.
#[derive(Debug, Clone, Copy)]
pub struct RateLimiterStats {
    /// Total operations that were allowed.
    pub total_allowed: u64,

    /// Total operations that were blocked.
    pub total_blocked: u64,

    /// Currently available tokens.
    pub available_tokens: f64,

    /// Maximum burst capacity.
    pub max_burst: u64,

    /// Tokens per second (sustained rate).
    pub refill_rate: f64,
}

impl RateLimiterStats {
    /// Returns the percentage of operations that were blocked.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Percentage calculation is acceptable
    pub fn block_rate(&self) -> f64 {
        // saturating_add: the sum only overflows after ~1.8e19 lifetime
        // operations, but an unchecked add would panic there in debug builds.
        let total = self.total_allowed.saturating_add(self.total_blocked);
        if total == 0 {
            0.0
        } else {
            (self.total_blocked as f64 / total as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn rate_limiter_allows_within_burst() {
        let limiter = RateLimiter::new(5, 1.0);

        // Should allow 5 operations
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }

        // 6th should be blocked
        assert!(!limiter.try_acquire());
    }

    #[test]
    fn rate_limiter_refills_over_time() {
        let limiter = RateLimiter::new(2, 10.0); // 10 tokens/second

        // Exhaust tokens
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());

        // Wait for refill (100ms = 1 token at 10/sec)
        thread::sleep(Duration::from_millis(150));

        // Should have refilled
        assert!(limiter.try_acquire());
    }

    #[test]
    fn rate_limiter_caps_at_max_burst() {
        let limiter = RateLimiter::new(3, 100.0);

        // Wait for tokens to accumulate
        thread::sleep(Duration::from_millis(100));

        // Should still only have max_burst tokens
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());
    }

    #[test]
    fn rate_limiter_would_allow_doesnt_consume() {
        let limiter = RateLimiter::new(1, 0.0);

        assert!(limiter.would_allow());
        assert!(limiter.would_allow()); // Still true

        assert!(limiter.try_acquire()); // Now consume
        assert!(!limiter.would_allow()); // Now false
    }

    #[test]
    fn rate_limiter_time_until_available() {
        let limiter = RateLimiter::new(1, 10.0);

        // With tokens available
        assert_eq!(limiter.time_until_available(), Duration::ZERO);

        // Exhaust tokens
        limiter.try_acquire();

        // Should need ~100ms for next token
        let wait_time = limiter.time_until_available();
        assert!(wait_time.as_millis() > 0);
        assert!(wait_time.as_millis() <= 150); // Some tolerance
    }

    #[test]
    fn rate_limiter_stats() {
        let limiter = RateLimiter::new(3, 1.0);

        limiter.try_acquire(); // allowed
        limiter.try_acquire(); // allowed
        limiter.try_acquire(); // allowed
        limiter.try_acquire(); // blocked
        limiter.try_acquire(); // blocked

        let stats = limiter.stats();
        assert_eq!(stats.total_allowed, 3);
        assert_eq!(stats.total_blocked, 2);
        assert_eq!(stats.max_burst, 3);
        assert!((stats.block_rate() - 40.0).abs() < 0.01);
    }

    #[test]
    fn rate_limiter_reset() {
        let limiter = RateLimiter::new(2, 0.0);

        // Exhaust and block some
        limiter.try_acquire();
        limiter.try_acquire();
        limiter.try_acquire();

        // Reset
        limiter.reset();

        let stats = limiter.stats();
        assert_eq!(stats.total_allowed, 0);
        assert_eq!(stats.total_blocked, 0);
        assert!(limiter.try_acquire());
    }

    #[test]
    fn rate_limiter_unlimited() {
        let limiter = RateLimiter::unlimited();

        // Should allow many operations
        for _ in 0..1000 {
            assert!(limiter.try_acquire());
        }
    }

    #[test]
    fn rate_limiter_default_for_writes() {
        let limiter = RateLimiter::default_for_writes();

        assert_eq!(limiter.max_burst, 120);
        assert!((limiter.refill_rate - 30.0).abs() < 0.01);
    }

    #[test]
    fn rate_limiter_default_impl_delegates_to_write_defaults() {
        // The Default impl just forwards to default_for_writes().
        let limiter = RateLimiter::default();
        assert_eq!(limiter.max_burst, 120);
        assert!((limiter.refill_rate - 30.0).abs() < 0.01);
    }

    #[test]
    fn block_rate_with_no_operations() {
        let limiter = RateLimiter::new(10, 1.0);
        let stats = limiter.stats();

        assert!((stats.block_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rate_limiter_zero_refill_time_until_available() {
        let limiter = RateLimiter::new(1, 0.0);

        // With tokens available, should be zero
        assert_eq!(limiter.time_until_available(), Duration::ZERO);

        // Exhaust tokens
        limiter.try_acquire();

        // With zero refill rate, should return Duration::MAX
        assert_eq!(limiter.time_until_available(), Duration::MAX);
    }

    #[test]
    fn time_until_available_saturates_on_tiny_refill_rate() {
        // A finite, positive but minuscule refill_rate passes config validation
        // (`Config::validate` only rejects non-finite/negative), yet makes
        // `seconds = 1.0 / refill_rate` overflow Duration's range. The method
        // must saturate to Duration::MAX rather than panic in from_secs_f64.
        let limiter = RateLimiter::new(1, f64::MIN_POSITIVE);

        // Drain the single token so the else-branch (refill_rate > 0) is taken.
        assert!(limiter.try_acquire());

        assert_eq!(limiter.time_until_available(), Duration::MAX);
    }

    #[test]
    fn refill_does_not_drain_tokens_with_negative_rate() {
        // A negative refill_rate is rejected by Config::validate, but the public
        // RateLimiter::new accepts it. refill() must not *remove* tokens
        // (`elapsed * negative` is negative); it no-ops for non-positive rates.
        let limiter = RateLimiter::new(3, -100.0);

        thread::sleep(Duration::from_millis(20));

        // Tokens stay at the full burst rather than draining below 3.
        assert!((limiter.available_tokens() - 3.0).abs() < f64::EPSILON);
        assert!(limiter.would_allow());
    }

    #[test]
    fn try_acquire_recovers_from_poisoned_tokens_mutex() {
        let limiter = RateLimiter::new(5, 1.0);

        // Poison the tokens mutex by panicking while holding it. catch_unwind
        // keeps the deliberate panic on this (output-captured) test thread.
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = limiter.tokens.lock().unwrap();
            panic!("intentionally poisoning the tokens mutex");
        }));
        assert!(poisoned.is_err());

        // lock_tokens() must recover via into_inner() rather than propagating
        // the poison (which would panic on .unwrap()).
        assert!(limiter.try_acquire());
    }

    #[test]
    fn refill_recovers_from_poisoned_last_refill_mutex() {
        let limiter = RateLimiter::new(5, 10.0);

        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = limiter.last_refill.lock().unwrap();
            panic!("intentionally poisoning the last_refill mutex");
        }));
        assert!(poisoned.is_err());

        // refill() (invoked by try_acquire) must recover the last_refill lock.
        assert!(limiter.try_acquire());
    }
}
