use std::time::{Duration, Instant};

use reqwest::{RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use thiserror::Error;

use oss_insight_http::RateLimitedClient;

pub struct SourceResponse<T> {
    pub body: String,
    pub elapsed: Duration,
    pub data: T,
}

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("http error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unexpected status {0}: {1}")]
    Status(StatusCode, String),
}

pub type SourceResult<T> = Result<T, SourceError>;

pub async fn fetch(
    client: &mut RateLimitedClient,
    builder: RequestBuilder,
) -> SourceResult<(String, Duration)> {
    let start = Instant::now();
    let resp = client.execute(builder).await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(SourceError::Status(status, body));
    }
    let body = resp.text().await?;
    Ok((body, start.elapsed()))
}

pub async fn get_json<T: DeserializeOwned>(
    client: &mut RateLimitedClient,
    builder: RequestBuilder,
) -> SourceResult<SourceResponse<T>> {
    let (body, elapsed) = fetch(client, builder).await?;
    let data = serde_json::from_str(&body)?;
    Ok(SourceResponse {
        body,
        elapsed,
        data,
    })
}
