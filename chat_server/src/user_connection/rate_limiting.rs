use std::time::{Duration, Instant};

// Security limits
pub const RATE_LIMIT_MESSAGES: usize = 10; // Max messages per window
pub const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(1); // 1 second window

// Simple rate limiter using token bucket
pub struct RateLimiter {
    tokens: usize,
    max_tokens: usize,
    last_refill: Instant,
    refill_interval: Duration,
}

impl RateLimiter {
    pub fn new(max_tokens: usize, refill_interval: Duration) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            last_refill: Instant::now(),
            refill_interval,
        }
    }

    pub fn check_and_consume(&mut self) -> bool {
        self.refill();
        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        if elapsed >= self.refill_interval {
            self.tokens = self.max_tokens;
            self.last_refill = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_messages_within_limit() {
        let mut limiter = RateLimiter::new(5, Duration::from_secs(1));

        // Should allow 5 messages
        for _ in 0..5 {
            assert!(limiter.check_and_consume());
        }
    }

    #[test]
    fn test_rate_limiter_blocks_excess_messages() {
        let mut limiter = RateLimiter::new(3, Duration::from_secs(1));

        // Consume all tokens
        for _ in 0..3 {
            assert!(limiter.check_and_consume());
        }

        // Should block the 4th message
        assert!(!limiter.check_and_consume());
    }

    #[test]
    fn test_rate_limiter_refills_after_interval() {
        let mut limiter = RateLimiter::new(2, Duration::from_millis(100));

        // Consume all tokens
        assert!(limiter.check_and_consume());
        assert!(limiter.check_and_consume());
        assert!(!limiter.check_and_consume());

        // Wait for refill
        std::thread::sleep(Duration::from_millis(150));

        // Should allow messages again
        assert!(limiter.check_and_consume());
        assert!(limiter.check_and_consume());
    }

    #[test]
    fn test_rate_limiter_multiple_refills() {
        let mut limiter = RateLimiter::new(1, Duration::from_millis(50));

        for _ in 0..3 {
            assert!(limiter.check_and_consume());
            assert!(!limiter.check_and_consume()); // Blocked
            std::thread::sleep(Duration::from_millis(60)); // Wait for refill
        }
    }
}
