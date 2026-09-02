use std::time::Duration;

use reqwest::{ClientBuilder, Result, header::ACCEPT};
use serde_json::Value;

use crate::crawler::RateLimitedClient;

const BASE_URL: &str = "https://api.ossinsight.io/v1";

const MEDIA_TYPE: &str = "application/json";

pub struct OssinsightBuilder;

impl OssinsightBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build(self) -> Ossinsight {
        Ossinsight {
            client: RateLimitedClient::new(
                ClientBuilder::new()
                    .user_agent(env!("CARGO_PKG_NAME"))
                    .build()
                    .unwrap(),
                Duration::from_secs(60),
                Duration::from_secs(3600),
            ),
        }
    }
}

impl Default for OssinsightBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Ossinsight {
    client: RateLimitedClient,
}

impl Ossinsight {
    pub async fn trends(&mut self, period: &str, lang: &str) -> Result<Value> {
        let builder = self
            .client
            .get(format!("{BASE_URL}/trends/repos/"))
            .query(&[("period", period), ("language", lang)])
            .header(ACCEPT, MEDIA_TYPE);
        let resp = self.client.execute(builder).await;
        let trends = resp.json().await?;
        Ok(trends)
    }
}
