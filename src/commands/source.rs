use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use oss_insight_source::{GithubBuilder, SearchOrder, SearchSort};

use crate::commands::config::Config;
use crate::commands::util::stdin_or_iter;

#[derive(Subcommand)]
pub enum SourceCommands {
    /// Source for GitHub.
    Github {
        #[command(subcommand)]
        command: GithubCommands,
    },
}

#[derive(Subcommand)]
pub enum GithubCommands {
    /// Prints repositories as JSON lines.
    Repo {
        #[command(flatten)]
        api: GithubRepoApi,
        /// Read from stdin.
        #[arg(long, group = "input")]
        stdin: bool,
        /// List of full_name or id.
        #[arg(group = "input")]
        key: Vec<String>,
    },
    /// Prints README of the repositories as JSON lines.
    Readme {
        #[command(flatten)]
        api: GithubRepoApi,
        /// Read from stdin.
        #[arg(long, group = "input")]
        stdin: bool,
        /// List of full_name or id.
        #[arg(group = "input")]
        key: Vec<String>,
    },
    /// Prints user profiles as JSON lines.
    User {
        #[command(flatten)]
        api: GithubUserApi,
        /// Read from stdin.
        #[arg(long, group = "input")]
        stdin: bool,
        /// List of login or id.
        #[arg(group = "input")]
        key: Vec<String>,
    },
    /// Prints weekly star history as JSON lines.
    StarHistory {
        #[command(flatten)]
        api: GithubRepoApi,
        /// Read from stdin.
        #[arg(long, group = "input")]
        stdin: bool,
        /// List of full_name or id.
        #[arg(group = "input")]
        key: Vec<String>,
    },
    /// Prints trending repositories as JSON lines.
    Trending,
    /// Prints repositories matching a search query as JSON lines.
    Search {
        /// GitHub search query, e.g. language:rust created:>2026-06-01.
        query: String,
        /// Sort field.
        #[arg(long)]
        sort: SearchSortArg,
        /// Sort order.
        #[arg(long)]
        order: SearchOrderArg,
        /// Maximum number of pages to fetch.
        #[arg(long)]
        max_pages: u32,
    },
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct GithubRepoApi {
    /// By full_name.
    #[arg(long, group = "api")]
    full_name: bool,
    /// By id.
    #[arg(long, group = "api")]
    id: bool,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct GithubUserApi {
    /// By login.
    #[arg(long, group = "api")]
    login: bool,
    /// By id.
    #[arg(long, group = "api")]
    id: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SearchSortArg {
    Stars,
    Forks,
    HelpWantedIssues,
    Updated,
}

impl SearchSortArg {
    fn into_sort(self) -> SearchSort {
        match self {
            SearchSortArg::Stars => SearchSort::Stars,
            SearchSortArg::Forks => SearchSort::Forks,
            SearchSortArg::HelpWantedIssues => SearchSort::HelpWantedIssues,
            SearchSortArg::Updated => SearchSort::Updated,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SearchOrderArg {
    Asc,
    Desc,
}

impl SearchOrderArg {
    fn into_order(self) -> SearchOrder {
        match self {
            SearchOrderArg::Asc => SearchOrder::Asc,
            SearchOrderArg::Desc => SearchOrder::Desc,
        }
    }
}

impl SourceCommands {
    pub async fn exec(&self, config: &Config) -> Result<()> {
        match self {
            SourceCommands::Github { command } => {
                let github_config = &config.source.github;
                let client_config = &config.http.client;
                let mut github_builder = GithubBuilder::new(
                    Duration::from_secs(client_config.min_delay_secs),
                    Duration::from_secs(client_config.max_delay_secs),
                    Duration::from_secs(client_config.max_retry_time_secs),
                    client_config.user_agent.clone(),
                );
                if let Some(token) = &github_config.token {
                    github_builder = github_builder.token(token.clone());
                }
                for path in &client_config.root_certificates {
                    github_builder = github_builder.add_root_certificate_path(path);
                }
                match command {
                    GithubCommands::Repo { api, stdin, key } => {
                        let mut github = github_builder.build();
                        let lines = stdin_or_iter(*stdin, key);
                        if api.full_name {
                            for line in lines {
                                let resp = github.repo(&line?).await?;
                                println!("{}", serde_json::to_string(&resp.data)?);
                            }
                        } else if api.id {
                            for line in lines {
                                let resp = github.repo_by_id(line?.parse()?).await?;
                                println!("{}", serde_json::to_string(&resp.data)?);
                            }
                        }
                    }
                    GithubCommands::Readme { api, stdin, key } => {
                        let mut github = github_builder.build();
                        let lines = stdin_or_iter(*stdin, key);
                        if api.full_name {
                            for line in lines {
                                let resp = github.readme(&line?).await?;
                                println!("{}", serde_json::to_string(&resp.data)?);
                            }
                        } else if api.id {
                            for line in lines {
                                let resp = github.readme_by_id(line?.parse()?).await?;
                                println!("{}", serde_json::to_string(&resp.data)?);
                            }
                        }
                    }
                    GithubCommands::User { api, stdin, key } => {
                        let mut github = github_builder.build();
                        let lines = stdin_or_iter(*stdin, key);
                        if api.login {
                            for line in lines {
                                let resp = github.user(&line?).await?;
                                println!("{}", serde_json::to_string(&resp.data)?);
                            }
                        } else if api.id {
                            for line in lines {
                                let resp = github.user_by_id(line?.parse()?).await?;
                                println!("{}", serde_json::to_string(&resp.data)?);
                            }
                        }
                    }
                    GithubCommands::StarHistory { api, stdin, key } => {
                        let mut github = github_builder.build();
                        let lines = stdin_or_iter(*stdin, key);
                        if api.full_name {
                            for line in lines {
                                let full_name = line?;
                                for page in 1.. {
                                    let history =
                                        github.stargazer_history(&full_name, page).await?.data;
                                    if history.is_empty() {
                                        break;
                                    }
                                    for week in history {
                                        println!("{}", serde_json::to_string(&week)?);
                                    }
                                }
                            }
                        } else if api.id {
                            for line in lines {
                                let id: u64 = line?.parse()?;
                                for page in 1.. {
                                    let history =
                                        github.stargazer_history_by_id(id, page).await?.data;
                                    if history.is_empty() {
                                        break;
                                    }
                                    for week in history {
                                        println!("{}", serde_json::to_string(&week)?);
                                    }
                                }
                            }
                        }
                    }
                    GithubCommands::Trending => {
                        let mut github = github_builder.build();
                        for lang in &github_config.trending.languages {
                            for period in &github_config.trending.periods {
                                let repos = github.trending(lang, period.as_str()).await?.data;
                                for repo in repos {
                                    println!("{}", serde_json::to_string(&repo)?);
                                }
                            }
                        }
                    }
                    GithubCommands::Search {
                        query,
                        sort,
                        order,
                        max_pages,
                    } => {
                        let mut github = github_builder.build();
                        let sort = sort.into_sort();
                        let order = order.into_order();
                        for page in 1..=*max_pages {
                            let search = github.search_repos(query, page, sort, order).await?;
                            if search.data.items.is_empty() {
                                break;
                            }
                            for repo in search.data.items {
                                println!("{}", serde_json::to_string(&repo)?);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
