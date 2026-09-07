use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use oss_insight_source::GithubBuilder;

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
    /// Prints trending repositories as JSON lines.
    Trending,
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
                }
            }
        }
        Ok(())
    }
}
