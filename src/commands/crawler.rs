use anyhow::Result;
use clap::{Args, Subcommand};
use oss_insight::crawler::GithubBuilder;

use crate::commands::util::stdin_or_iter;

#[derive(Subcommand)]
pub enum CrawlerCommands {
    /// Crawler for GitHub.
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
    /// Prints stargazers of the repo as JSON lines.
    Stargazers { full_name: String },
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

impl CrawlerCommands {
    pub async fn exec(&self) -> Result<()> {
        match self {
            CrawlerCommands::Github { token, command } => {
                let github_builder = if let Some(token) = token {
                    GithubBuilder::new().token(String::from(token))
                } else {
                    GithubBuilder::new()
                };
                match command {
                    GithubCommands::Stargazers { full_name } => {
                        let mut github = github_builder.build();
                        for page in 1.. {
                            let stargazers = github.repos_stargazers(full_name, page).await?;
                            if stargazers.is_empty() {
                                break;
                            }
                            for stargazer in stargazers {
                                println!("{}", stargazer);
                            }
                        }
                    }
                    GithubCommands::Repo { api, stdin, key } => {
                        let mut github = github_builder.build();
                        let lines = stdin_or_iter(*stdin, key);
                        if api.full_name {
                            for line in lines {
                                println!("{}", github.repo(&line?).await?);
                            }
                        } else if api.id {
                            for line in lines {
                                println!("{}", github.repo_by_id(line?.parse()?).await?);
                            }
                        }
                    }
                    GithubCommands::Readme { api, stdin, key } => {
                        let mut github = github_builder.build();
                        let lines = stdin_or_iter(*stdin, key);
                        if api.full_name {
                            for line in lines {
                                println!("{}", github.readme(&line?).await?);
                            }
                        } else if api.id {
                            for line in lines {
                                println!("{}", github.readme_by_id(line?.parse()?).await?);
                            }
                        }
                    }
                    GithubCommands::User { api, stdin, key } => {
                        let mut github = github_builder.build();
                        let lines = stdin_or_iter(*stdin, key);
                        if api.login {
                            for line in lines {
                                println!("{}", github.user(&line?).await?);
                            }
                        } else if api.id {
                            for line in lines {
                                println!("{}", github.user_by_id(line?.parse()?).await?);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
