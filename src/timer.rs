use std::time::Duration;

use tokio::time::{Instant, Sleep, sleep_until};

pub struct ExponentialBackoffTimer {
    deadline: Instant,
    delay: Duration,
    min_delay: Duration,
    max_delay: Duration,
}

impl ExponentialBackoffTimer {
    pub fn new(deadline: Instant, min_delay: Duration, max_delay: Duration) -> Self {
        Self {
            deadline,
            delay: min_delay,
            min_delay,
            max_delay,
        }
    }

    pub fn sleep(&self) -> Sleep {
        sleep_until(self.deadline)
    }

    pub fn set_deadline(&mut self, new_deadline: Instant) {
        self.deadline = new_deadline;
        self.delay = self.min_delay;
    }

    pub fn backoff(&mut self) {
        self.deadline += self.delay;
        self.delay = (2 * self.delay).min(self.max_delay);
    }
}
