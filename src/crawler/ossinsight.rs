use std::time::Duration;

use log::warn;
use reqwest::{
    Client, ClientBuilder, RequestBuilder, Response, Result, StatusCode, header::ACCEPT,
};
use serde_json::Value;
use tokio::time::Instant;

use crate::timer::ExponentialBackoffTimer;

const BASE_URL: &str = "https://api.ossinsight.io/v1";

const MEDIA_TYPE: &str = "application/json";

pub struct OssinsightBuilder;

impl OssinsightBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build(self) -> Ossinsight {
        Ossinsight {
            client: ClientBuilder::new()
                .user_agent(env!("CARGO_PKG_NAME"))
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

pub struct Ossinsight {
    client: Client,
    timer: ExponentialBackoffTimer,
}

async fn send_with_retry(builder: RequestBuilder, timer: &mut ExponentialBackoffTimer) -> Response {
    loop {
        let req = builder.try_clone().unwrap();
        timer.sleep().await;
        if let Ok(resp) = req.send().await {
            if resp.status() == StatusCode::OK {
                return resp;
            }
            timer.backoff();
            warn!("{:?}", resp);
        }
    }
}

impl Ossinsight {
    pub async fn trends(&mut self, period: &str, lang: &str) -> Result<Value> {
        let builder = self
            .client
            .get(format!("{BASE_URL}/trends/repos/"))
            .query(&[("period", period), ("language", lang)])
            .header(ACCEPT, MEDIA_TYPE);
        let resp = send_with_retry(builder, &mut self.timer).await;
        let trends = resp.json().await?;
        Ok(trends)
    }
}
