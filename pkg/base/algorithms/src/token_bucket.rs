use std::time::{Duration, Instant};

pub struct TokenBucket {
    remaining_tokens: usize,
    max_tokens: usize,
    time_per_token: Duration,
    last_time: Instant,
}

impl TokenBucket {
    pub fn new(max_tokens: usize, refresh_window: Duration) -> Self {
        let time_per_token = refresh_window / (max_tokens as u32);

        Self {
            remaining_tokens: max_tokens,
            max_tokens,
            time_per_token,
            last_time: Instant::now(),
        }
    }

    pub fn time_per_token(&self) -> Duration {
        self.time_per_token
    }

    /// Take 'num' tokens from the bucket.
    ///
    /// On success, returns true.
    /// If there are not enough tokens at this time, does nothing and returns false.
    pub fn take(&mut self, num: usize) -> bool {
        // Refill
        let now = Instant::now();
        let increment = ((now - self.last_time).as_micros() as u64)
            / (self.time_per_token.as_micros() as u64);
        self.last_time += self.time_per_token * (increment as u32);
        self.remaining_tokens = core::cmp::min(
            self.max_tokens,
            self.remaining_tokens + (increment as usize),
        );

        if self.remaining_tokens < num {
            return false;
        }

        self.remaining_tokens -= num;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO
}
