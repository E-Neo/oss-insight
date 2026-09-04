use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use oss_insight_source::GithubBuilder;

use crate::commands::util::stdin_or_iter;

#[derive(Subcommand)]
pub enum SourceCommands {
    /// Source for GitHub.
    Github {
        /// GitHub token.
        #[arg(long)]
        token: Option<String>,
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
    Trending {
        /// Period of trending repositories.
        #[arg(long)]
        period: GithubPeriod,
        /// Language of trending repositories.
        lang: String,
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

#[derive(Clone, ValueEnum)]
pub enum GithubPeriod {
    #[value(name = "daily")]
    Daily,
    #[value(name = "weekly")]
    Weekly,
    #[value(name = "monthly")]
    Monthly,
}

impl SourceCommands {
    pub async fn exec(&self) -> Result<()> {
        match self {
            SourceCommands::Github { token, command } => {
                let github_builder = if let Some(token) = token {
                    GithubBuilder::new().token(String::from(token))
                } else {
                    GithubBuilder::new()
                };
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
                    GithubCommands::Trending { period, lang } => {
                        let mut github = github_builder.build();
                        let repos = github
                            .trending(lang, period.to_possible_value().unwrap().get_name())
                            .await?
                            .data;
                        for repo in repos {
                            println!("{}", serde_json::to_string(&repo)?);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
