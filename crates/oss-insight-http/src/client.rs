use std::ops::Deref;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{
    Client, ClientBuilder, Proxy, RequestBuilder, Response, StatusCode, header::HeaderMap,
};
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
                Ok(mut resp) => {
                    let retry_after = get_retry_after(&resp);
                    if let Some(retry_after) = retry_after {
                        self.timer.set_deadline(Instant::now() + retry_after);
                    }
                    if let Some(new_deadline) = get_x_ratelimit_reset(&resp) {
                        self.timer.set_deadline(new_deadline);
                    }
                    if resp.status() == StatusCode::OK {
                        return Ok(resp);
                    }
                    let status = resp.status();
                    let url = resp.url().to_string();
                    let retry_after_secs = retry_after.map(|d| d.as_secs());
                    let ratelimit_limit = header_u64(&resp, "x-ratelimit-limit");
                    let ratelimit_remaining = header_u64(&resp, "x-ratelimit-remaining");
                    let ratelimit_reset = header_u64(&resp, "x-ratelimit-reset");
                    let mut body = String::new();
                    while let Some(chunk) = resp.chunk().await.ok().flatten() {
                        body.push_str(&String::from_utf8_lossy(&chunk));
                    }
                    last_response = Some(resp);
                    self.timer.backoff();
                    warn!(
                        url = %url,
                        ?status,
                        ?retry_after_secs,
                        ?ratelimit_limit,
                        ?ratelimit_remaining,
                        ?ratelimit_reset,
                        body = %body,
                        "non-200 response, backing off"
                    );
                }
                Err(error) => {
                    self.timer.backoff();
                    warn!(?error, "transport error, backing off");
                    last_error = Some(error);
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
    http_proxy: Option<String>,
    https_proxy: Option<String>,
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
            http_proxy: None,
            https_proxy: None,
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

    pub fn http_proxy(mut self, url: impl Into<String>) -> Self {
        self.http_proxy = Some(url.into());
        self
    }

    pub fn https_proxy(mut self, url: impl Into<String>) -> Self {
        self.https_proxy = Some(url.into());
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
        if let Some(http_proxy) = &self.http_proxy {
            builder = builder.proxy(
                Proxy::http(http_proxy).unwrap_or_else(|e| panic!("invalid http_proxy: {e}")),
            );
        }
        if let Some(https_proxy) = &self.https_proxy {
            builder = builder.proxy(
                Proxy::https(https_proxy).unwrap_or_else(|e| panic!("invalid https_proxy: {e}")),
            );
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

fn header_u64(resp: &Response, name: &str) -> Option<u64> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

fn get_retry_after(resp: &Response) -> Option<Duration> {
    header_u64(resp, "retry-after").map(Duration::from_secs)
}

fn get_x_ratelimit_reset(resp: &Response) -> Option<Instant> {
    header_u64(resp, "x-ratelimit-remaining")
        .filter(|remaining| *remaining == 0)
        .and_then(|_| header_u64(resp, "x-ratelimit-reset"))
        .map(|secs| {
            Instant::now()
                + (UNIX_EPOCH + Duration::from_secs(secs))
                    .duration_since(SystemTime::now())
                    .unwrap_or(Duration::ZERO)
        })
}
