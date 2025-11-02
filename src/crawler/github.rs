use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::warn;
use reqwest::{
    Client, ClientBuilder, IntoUrl, RequestBuilder, Response, Result, StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap},
};
use serde_json::Value;
use tokio::time::Instant;

use crate::timer::ExponentialBackoffTimer;

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
            timer: ExponentialBackoffTimer::new(
                Instant::now(),
                Duration::from_secs(60),
                Duration::from_secs(3600),
            ),
        }
    }
}

pub struct Github {
    client: Client,
    timer: ExponentialBackoffTimer,
}

fn get_retry_after(resp: &Response) -> Option<Duration> {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| Duration::from_secs(secs))
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

async fn send_with_retry(builder: RequestBuilder, timer: &mut ExponentialBackoffTimer) -> Response {
    loop {
        let req = builder.try_clone().unwrap();
        timer.sleep().await;
        if let Ok(resp) = req.send().await {
            if let Some(retry_after) = get_retry_after(&resp) {
                timer.set_deadline(Instant::now() + retry_after);
            }
            if let Some(new_deadline) = get_x_ratelimit_reset(&resp) {
                timer.set_deadline(new_deadline);
            }
            if resp.status() == StatusCode::OK {
                return resp;
            }
            timer.backoff();
            warn!("{:?}", resp);
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
        let resp = send_with_retry(builder, &mut self.timer).await;
        let stargazers = resp.json().await?;
        Ok(stargazers)
    }

    async fn get<U: IntoUrl>(&mut self, url: U) -> Result<Value> {
        let builder = self.client.get(url).header(ACCEPT, MEDIA_TYPE_DEFAULT);
        let resp = send_with_retry(builder, &mut self.timer).await;
        let value = resp.json().await?;
        Ok(value)
    }

    pub async fn repo(&mut self, full_name: &str) -> Result<Value> {
        self.get(format!("{BASE_URL}/repos/{full_name}")).await
    }

    pub async fn repo_by_id(&mut self, id: u64) -> Result<Value> {
        self.get(format!("{BASE_URL}/repositories/{id}")).await
    }

    pub async fn readme(&mut self, full_name: &str) -> Result<Value> {
        self.get(format!("{BASE_URL}/repos/{full_name}/readme"))
            .await
    }

    pub async fn readme_by_id(&mut self, id: u64) -> Result<Value> {
        self.get(format!("{BASE_URL}/repositories/{id}/readme"))
            .await
    }

    pub async fn user(&mut self, login: &str) -> Result<Value> {
        self.get(format!("{BASE_URL}/users/{login}")).await
    }

    pub async fn user_by_id(&mut self, id: u64) -> Result<Value> {
        self.get(format!("{BASE_URL}/user/{id}")).await
    }
}
