use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub source: SourceConfig,
    pub http: HttpConfig,
    pub db: DbConfig,
    pub workflow: WorkflowConfig,
}

#[derive(Debug, Deserialize)]
pub struct SourceConfig {
    pub github: GithubConfig,
}

#[derive(Debug, Deserialize)]
pub struct GithubConfig {
    pub token: Option<String>,
    pub trending: TrendingConfig,
}

#[derive(Debug, Deserialize)]
pub struct TrendingConfig {
    pub periods: Vec<GithubPeriod>,
    pub languages: Vec<String>,
    pub search: TrendingSearchConfig,
}

#[derive(Debug, Deserialize)]
pub struct TrendingSearchConfig {
    pub stars: String,
    pub created: String,
    pub max_pages: u32,
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
    #[serde(default)]
    pub root_certificates: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct DbConfig {
    pub path: String,
    pub ttl_secs: u64,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowConfig {
    pub ttl_secs: u64,
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

    pub fn db_path(&self) -> Result<PathBuf> {
        if self.db.path.is_empty() {
            anyhow::bail!("[db] path must not be empty");
        }
        Ok(home_dir().join(&self.db.path))
    }

    pub fn resolve_home_path(&self, relative: &str) -> PathBuf {
        home_dir().join(relative)
    }

    pub fn log_dir() -> PathBuf {
        home_dir().join("log")
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GithubPeriod {
    Daily,
    Weekly,
    Monthly,
}

impl GithubPeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            GithubPeriod::Daily => "daily",
            GithubPeriod::Weekly => "weekly",
            GithubPeriod::Monthly => "monthly",
        }
    }
}

fn home_dir() -> PathBuf {
    match env::var("OSS_INSIGHT_HOME") {
        Ok(home) => PathBuf::from(home),
        Err(_) => env::home_dir()
            .expect("cannot determine home directory")
            .join(".oss-insight"),
    }
}
