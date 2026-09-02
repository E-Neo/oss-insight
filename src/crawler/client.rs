use std::ops::Deref;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::warn;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tokio::time::{Instant, Sleep, sleep_until};

struct ExponentialBackoffTimer {
    deadline: Instant,
    delay: Duration,
    min_delay: Duration,
    max_delay: Duration,
}

impl ExponentialBackoffTimer {
    fn new(deadline: Instant, min_delay: Duration, max_delay: Duration) -> Self {
        Self {
            deadline,
            delay: min_delay,
            min_delay,
            max_delay,
        }
    }

    fn sleep(&self) -> Sleep {
        sleep_until(self.deadline)
    }

    fn set_deadline(&mut self, new_deadline: Instant) {
        self.deadline = new_deadline;
        self.delay = self.min_delay;
    }

    fn backoff(&mut self) {
        self.deadline += self.delay;
        self.delay = (2 * self.delay).min(self.max_delay);
    }
}

pub struct RateLimitedClient {
    client: Client,
    timer: ExponentialBackoffTimer,
}

impl RateLimitedClient {
    pub fn new(client: Client, min_delay: Duration, max_delay: Duration) -> Self {
        Self {
            client,
            timer: ExponentialBackoffTimer::new(Instant::now(), min_delay, max_delay),
        }
    }

    pub async fn execute(&mut self, builder: RequestBuilder) -> Response {
        loop {
            let req = builder.try_clone().unwrap();
            self.timer.sleep().await;
            if let Ok(resp) = req.send().await {
                if let Some(retry_after) = get_retry_after(&resp) {
                    self.timer.set_deadline(Instant::now() + retry_after);
                }
                if let Some(new_deadline) = get_x_ratelimit_reset(&resp) {
                    self.timer.set_deadline(new_deadline);
                }
                if resp.status() == StatusCode::OK {
                    return resp;
                }
                self.timer.backoff();
                warn!("{:?}", resp);
            }
        }
    }
}

impl Deref for RateLimitedClient {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

fn get_retry_after(resp: &Response) -> Option<Duration> {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn get_x_ratelimit_reset(resp: &Response) -> Option<Instant> {
    let headers = resp.headers();
    headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|remaining| *remaining == 0)
        .and_then(|_| {
            headers
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(|secs| {
                    Instant::now()
                        + (UNIX_EPOCH + Duration::from_secs(secs))
                            .duration_since(SystemTime::now())
                            .unwrap_or(Duration::ZERO)
                })
        })
}
