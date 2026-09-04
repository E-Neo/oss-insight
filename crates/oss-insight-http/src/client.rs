use std::ops::Deref;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{Client, ClientBuilder, RequestBuilder, Response, StatusCode, header::HeaderMap};
use tokio::time::{Instant, Sleep, sleep_until};
use tracing::warn;

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

    fn sleep(&self, limit: Instant) -> Sleep {
        sleep_until(self.deadline.min(limit))
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
    max_retry_time: Duration,
}

impl RateLimitedClient {
    pub async fn execute(&mut self, builder: RequestBuilder) -> reqwest::Result<Response> {
        let deadline = Instant::now() + self.max_retry_time;
        let mut last_response: Option<Response> = None;
        let mut last_error: Option<reqwest::Error> = None;
        loop {
            let req = builder.try_clone().unwrap();
            self.timer.sleep(deadline).await;
            match req.send().await {
                Ok(resp) => {
                    if let Some(retry_after) = get_retry_after(&resp) {
                        self.timer.set_deadline(Instant::now() + retry_after);
                    }
                    if let Some(new_deadline) = get_x_ratelimit_reset(&resp) {
                        self.timer.set_deadline(new_deadline);
                    }
                    if resp.status() == StatusCode::OK {
                        return Ok(resp);
                    }
                    let status = resp.status();
                    last_response = Some(resp);
                    self.timer.backoff();
                    warn!(?status, "non-200 response, backing off");
                }
                Err(error) => {
                    last_error = Some(error);
                    self.timer.backoff();
                    warn!("transport error, backing off");
                }
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        match last_response {
            Some(resp) => Ok(resp),
            None => Err(last_error.unwrap()),
        }
    }
}

impl Deref for RateLimitedClient {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

pub struct RateLimitedClientBuilder {
    min_delay: Duration,
    max_delay: Duration,
    max_retry_time: Duration,
    user_agent: Option<String>,
    root_certificates: Vec<reqwest::Certificate>,
    headers: HeaderMap,
}

impl RateLimitedClientBuilder {
    pub fn new(min_delay: Duration, max_delay: Duration, max_retry_time: Duration) -> Self {
        Self {
            min_delay,
            max_delay,
            max_retry_time,
            user_agent: None,
            root_certificates: Vec::new(),
            headers: HeaderMap::new(),
        }
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn add_root_certificate(mut self, certificate: reqwest::Certificate) -> Self {
        self.root_certificates.push(certificate);
        self
    }

    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    pub fn build(self) -> RateLimitedClient {
        let mut builder = ClientBuilder::new();
        if let Some(user_agent) = self.user_agent {
            builder = builder.user_agent(user_agent);
        }
        for certificate in self.root_certificates {
            builder = builder.add_root_certificate(certificate);
        }
        if !self.headers.is_empty() {
            builder = builder.default_headers(self.headers);
        }
        RateLimitedClient {
            client: builder.build().unwrap(),
            timer: ExponentialBackoffTimer::new(Instant::now(), self.min_delay, self.max_delay),
            max_retry_time: self.max_retry_time,
        }
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
