use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{
    Client, ClientBuilder, RequestBuilder, Response, Result, StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap},
};
use serde_json::Value;
use tokio::time::{Instant, sleep_until};

const BASE_URL: &str = "https://api.github.com";

const MEDIA_TYPE_DEFAULT: &str = "application/vnd.github+json";
const MEDIA_TYPE_STAR: &str = "application/vnd.github.star+json";

const PER_PAGE: u32 = 100;

pub struct GithubBuilder {
    token: Option<String>,
}

impl GithubBuilder {
    pub fn new() -> Self {
        Self { token: None }
    }

    pub fn token(self, t: String) -> Self {
        Self { token: Some(t) }
    }

    pub fn build(self) -> Github {
        let mut headers = HeaderMap::new();
        if let Some(token) = self.token {
            headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        }
        Github {
            client: ClientBuilder::new()
                .user_agent(env!("CARGO_PKG_NAME"))
                .default_headers(headers)
                .build()
                .unwrap(),
            deadline: Instant::now(),
        }
    }
}

pub struct Github {
    client: Client,
    deadline: Instant,
}

async fn send_with_retry(builder: RequestBuilder, deadline: Instant) -> (Response, Instant) {
    let mut deadline = deadline;
    let mut retry_after = Duration::ZERO;
    loop {
        if let Some(req) = builder.try_clone() {
            sleep_until(deadline + retry_after).await;
            if let Ok(resp) = req.send().await {
                if let Some(new_deadline) = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|secs| Instant::now() + Duration::from_secs(secs))
                {
                    deadline = new_deadline;
                    retry_after = Duration::ZERO;
                }
                if let Some(0) = resp
                    .headers()
                    .get("x-ratelimit-remaining")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    if let Some(new_deadline) = resp
                        .headers()
                        .get("x-ratelimit-reset")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|secs| {
                            Instant::now()
                                + (UNIX_EPOCH + Duration::from_secs(secs))
                                    .duration_since(SystemTime::now())
                                    .unwrap_or(Duration::ZERO)
                        })
                    {
                        deadline = new_deadline;
                        retry_after = Duration::ZERO;
                    }
                }
                if resp.status() == StatusCode::OK {
                    return (resp, deadline);
                } else {
                    retry_after = (2 * retry_after).max(Duration::from_secs(60));
                    deadline = Instant::now() + retry_after;
                }
            }
        }
    }
}

impl Github {
    pub async fn repos_stargazers(&mut self, full_name: &str, page: u32) -> Result<Vec<Value>> {
        let builder = self
            .client
            .get(format!("{BASE_URL}/repos/{full_name}/stargazers"))
            .query(&[("per_page", PER_PAGE), ("page", page)])
            .header(ACCEPT, MEDIA_TYPE_STAR);
        let (resp, deadline) = send_with_retry(builder, self.deadline).await;
        self.deadline = deadline;
        let stargazers = resp.json().await?;
        Ok(stargazers)
    }

    pub async fn user(&mut self, id: u64) -> Result<Value> {
        let builder = self
            .client
            .get(format!("{BASE_URL}/user/{id}"))
            .header(ACCEPT, MEDIA_TYPE_DEFAULT);
        let (resp, deadline) = send_with_retry(builder, self.deadline).await;
        self.deadline = deadline;
        let user = resp.json().await?;
        Ok(user)
    }
}
