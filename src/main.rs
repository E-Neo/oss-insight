use std::io::{self, BufRead};

use anyhow::Result;
use clap::{Parser, Subcommand};
use oss_insight::crawler::GithubBuilder;

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
enum GithubCommands {
    /// Prints stargazers of the repo as JSON lines.
    Stargazers { full_name: String },
    /// Read account IDs from stdin and prints their profiles as JSON lines.
    Users,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Github { token, command } => {
            let mut github = if let Some(token) = token {
                GithubBuilder::new().token(String::from(token)).build()
            } else {
                GithubBuilder::new().build()
            };
            match command {
                GithubCommands::Stargazers { full_name } => {
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
                GithubCommands::Users => {
                    for line in io::stdin().lock().lines() {
                        let user = github.user(line?.parse()?).await?;
                        println!("{}", user);
                    }
                }
            }
        }
    }
    Ok(())
}
