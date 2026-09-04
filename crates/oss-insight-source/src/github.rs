use std::time::Duration;

use reqwest::{
    IntoUrl,
    header::{ACCEPT, AUTHORIZATION, HeaderMap},
};
use scraper::{ElementRef, Html, Selector};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use oss_insight_http::{RateLimitedClient, RateLimitedClientBuilder};

use crate::response::{SourceResponse, SourceResult, fetch, get_json};

const BASE_URL: &str = "https://api.github.com";
const TRENDING_BASE_URL: &str = "https://github.com/trending";

const MEDIA_TYPE_DEFAULT: &str = "application/vnd.github+json";

pub struct GithubBuilder {
    token: Option<String>,
}

#[allow(clippy::new_without_default)]
impl GithubBuilder {
    pub fn new() -> Self {
        Self { token: None }
    }

    pub fn token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }

    pub fn build(self) -> Github {
        let mut headers = HeaderMap::new();
        if let Some(token) = self.token {
            headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        }
        Github {
            client: RateLimitedClientBuilder::new(
                Duration::from_secs(60),
                Duration::from_secs(3600),
                Duration::from_secs(1800),
            )
            .user_agent(env!("CARGO_PKG_NAME"))
            .default_headers(headers)
            .build(),
        }
    }
}

pub struct Github {
    client: RateLimitedClient,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimpleUser {
    pub login: String,
    pub id: u64,
    #[serde(default)]
    pub node_id: Option<String>,
    pub avatar_url: String,
    #[serde(default)]
    pub gravatar_id: Option<String>,
    pub html_url: String,
    pub r#type: String,
    #[serde(default)]
    pub user_view_type: Option<String>,
    pub site_admin: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct License {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub spdx_id: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repo {
    pub id: u64,
    #[serde(default)]
    pub node_id: Option<String>,
    pub name: String,
    pub full_name: String,
    pub owner: SimpleUser,
    pub private: bool,
    pub html_url: String,
    #[serde(default)]
    pub description: Option<String>,
    pub fork: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub size: u64,
    pub stargazers_count: u64,
    pub watchers_count: u64,
    #[serde(default)]
    pub language: Option<String>,
    pub forks_count: u64,
    pub open_issues_count: u64,
    #[serde(default)]
    pub mirror_url: Option<String>,
    pub archived: bool,
    pub disabled: bool,
    #[serde(default)]
    pub license: Option<License>,
    pub allow_forking: bool,
    pub is_template: bool,
    pub web_commit_signoff_required: bool,
    #[serde(default)]
    pub topics: Vec<String>,
    pub visibility: String,
    pub forks: u64,
    pub open_issues: u64,
    pub watchers: u64,
    pub default_branch: String,
    #[serde(default)]
    pub temp_clone_token: Option<String>,
    #[serde(default)]
    pub custom_properties: Option<Value>,
    #[serde(default)]
    pub organization: Option<SimpleUser>,
    pub network_count: u64,
    pub subscribers_count: u64,
    pub has_issues: bool,
    pub has_projects: bool,
    pub has_downloads: bool,
    pub has_wiki: bool,
    pub has_pages: bool,
    pub has_discussions: bool,
    pub has_pull_requests: bool,
    #[serde(default)]
    pub pull_request_creation_policy: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub login: String,
    pub id: u64,
    #[serde(default)]
    pub node_id: Option<String>,
    pub avatar_url: String,
    #[serde(default)]
    pub gravatar_id: Option<String>,
    pub html_url: String,
    pub r#type: String,
    #[serde(default)]
    pub user_view_type: Option<String>,
    pub site_admin: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub blog: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub hireable: Option<bool>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub twitter_username: Option<String>,
    pub public_repos: u64,
    pub public_gists: u64,
    pub followers: u64,
    pub following: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Readme {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub size: u64,
    pub html_url: String,
    pub r#type: String,
    pub content: String,
    pub encoding: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrendingRepo {
    pub full_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub language: String,
    pub stars: u64,
    pub forks: u64,
    pub stars_this_period: u64,
}

impl Github {
    pub async fn repo(&mut self, full_name: &str) -> SourceResult<SourceResponse<Repo>> {
        self.get(format!("{BASE_URL}/repos/{full_name}")).await
    }

    pub async fn repo_by_id(&mut self, id: u64) -> SourceResult<SourceResponse<Repo>> {
        self.get(format!("{BASE_URL}/repositories/{id}")).await
    }

    pub async fn readme(&mut self, full_name: &str) -> SourceResult<SourceResponse<Readme>> {
        self.get(format!("{BASE_URL}/repos/{full_name}/readme"))
            .await
    }

    pub async fn readme_by_id(&mut self, id: u64) -> SourceResult<SourceResponse<Readme>> {
        self.get(format!("{BASE_URL}/repositories/{id}/readme"))
            .await
    }

    pub async fn user(&mut self, login: &str) -> SourceResult<SourceResponse<User>> {
        self.get(format!("{BASE_URL}/users/{login}")).await
    }

    pub async fn user_by_id(&mut self, id: u64) -> SourceResult<SourceResponse<User>> {
        self.get(format!("{BASE_URL}/user/{id}")).await
    }

    pub async fn trending(
        &mut self,
        lang: &str,
        since: &str,
    ) -> SourceResult<SourceResponse<Vec<TrendingRepo>>> {
        let url = if lang.is_empty() {
            TRENDING_BASE_URL.to_string()
        } else {
            format!("{TRENDING_BASE_URL}/{lang}")
        };
        let builder = self
            .client
            .get(url)
            .query(&[("since", since)])
            .header(ACCEPT, MEDIA_TYPE_DEFAULT);
        let (body, elapsed) = fetch(&mut self.client, builder).await?;
        let data = parse_trending(&body);
        Ok(SourceResponse {
            body,
            elapsed,
            data,
        })
    }

    async fn get<U: IntoUrl, T: DeserializeOwned>(
        &mut self,
        url: U,
    ) -> SourceResult<SourceResponse<T>> {
        let builder = self.client.get(url).header(ACCEPT, MEDIA_TYPE_DEFAULT);
        get_json(&mut self.client, builder).await
    }
}

fn parse_trending(html: &str) -> Vec<TrendingRepo> {
    let document = Html::parse_document(html);
    let article = Selector::parse("article.Box-row").unwrap();
    let name = Selector::parse("h2 a").unwrap();
    let description = Selector::parse("p.col-9").unwrap();
    let language = Selector::parse("[itemprop='programmingLanguage']").unwrap();
    let stars = Selector::parse("a[href$='/stargazers']").unwrap();
    let forks = Selector::parse("a[href$='/forks']").unwrap();
    let period = Selector::parse("span.d-inline-block.float-sm-right").unwrap();

    document
        .select(&article)
        .filter_map(|repo| {
            let full_name = repo.select(&name).next()?.attr("href")?;
            Some(TrendingRepo {
                full_name: full_name.trim_start_matches('/').to_string(),
                description: text_of(repo.select(&description).next()),
                language: text_of(repo.select(&language).next()),
                stars: number_of(repo.select(&stars).next()),
                forks: number_of(repo.select(&forks).next()),
                stars_this_period: number_of(repo.select(&period).next()),
            })
        })
        .collect()
}

fn text_of(element: Option<ElementRef>) -> String {
    element
        .map(|e| e.text().collect::<String>())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn number_of(element: Option<ElementRef>) -> u64 {
    element
        .and_then(|e| {
            e.text()
                .collect::<String>()
                .split_whitespace()
                .next()
                .and_then(|n| n.replace(',', "").parse().ok())
        })
        .unwrap_or(0)
}
