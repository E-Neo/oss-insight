use std::time::Duration;

use reqwest::{
    ClientBuilder, IntoUrl, Result,
    header::{ACCEPT, AUTHORIZATION, HeaderMap},
};
use scraper::{ElementRef, Html, Selector};
use serde_json::{Value, json};

use crate::crawler::RateLimitedClient;

const BASE_URL: &str = "https://api.github.com";
const TRENDING_BASE_URL: &str = "https://github.com/trending";

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
            client: RateLimitedClient::new(
                ClientBuilder::new()
                    .user_agent(env!("CARGO_PKG_NAME"))
                    .default_headers(headers)
                    .build()
                    .unwrap(),
                Duration::from_secs(60),
                Duration::from_secs(3600),
            ),
        }
    }
}

impl Default for GithubBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Github {
    client: RateLimitedClient,
}

impl Github {
    pub async fn repos_stargazers(&mut self, full_name: &str, page: u32) -> Result<Vec<Value>> {
        let builder = self
            .client
            .get(format!("{BASE_URL}/repos/{full_name}/stargazers"))
            .query(&[("per_page", PER_PAGE), ("page", page)])
            .header(ACCEPT, MEDIA_TYPE_STAR);
        let resp = self.client.execute(builder).await;
        let stargazers = resp.json().await?;
        Ok(stargazers)
    }

    pub async fn trending(&mut self, lang: &str, since: &str) -> Result<Vec<Value>> {
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
        let resp = self.client.execute(builder).await;
        let html = resp.text().await?;
        Ok(parse_trending(&html))
    }

    async fn get<U: IntoUrl>(&mut self, url: U) -> Result<Value> {
        let builder = self.client.get(url).header(ACCEPT, MEDIA_TYPE_DEFAULT);
        let resp = self.client.execute(builder).await;
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

fn parse_trending(html: &str) -> Vec<Value> {
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
            Some(json!({
                "full_name": full_name.trim_start_matches('/'),
                "description": text_of(repo.select(&description).next()),
                "language": text_of(repo.select(&language).next()),
                "stars": number_of(repo.select(&stars).next()),
                "forks": number_of(repo.select(&forks).next()),
                "stars_this_period": number_of(repo.select(&period).next()),
            }))
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
