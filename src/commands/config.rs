use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub source: SourceConfig,
    pub http: HttpConfig,
}

#[derive(Debug, Deserialize)]
pub struct SourceConfig {
    pub github: GithubConfig,
}

#[derive(Debug, Deserialize)]
pub struct GithubConfig {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HttpConfig {
    pub client: HttpClientConfig,
}

#[derive(Debug, Deserialize)]
pub struct HttpClientConfig {
    pub user_agent: String,
    pub min_delay_secs: u64,
    pub max_delay_secs: u64,
    pub max_retry_time_secs: u64,
    pub root_certificates: Vec<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let home = home_dir();
        let path = home.join("config.toml");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config =
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }
}

fn home_dir() -> PathBuf {
    match env::var("OSS_INSIGHT_HOME") {
        Ok(home) => PathBuf::from(home),
        Err(_) => {
            let home = env::var("HOME").expect("neither OSS_INSIGHT_HOME nor HOME is set");
            PathBuf::from(home).join(".oss-insight")
        }
    }
}
