use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::{config::Config, github_trending, source::SourceCommands, workflow};

#[derive(Parser)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Information source.
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    /// Runs the github-trending workflow.
    Workflow,
    /// Fetches github trending ids without storing anything.
    GithubTrending {
        /// Write the repo ids to this file, one per line.
        #[arg(long)]
        output: PathBuf,
    },
}

impl Cli {
    pub async fn exec(&self) -> Result<()> {
        let config = Config::load()?;
        match &self.command {
            Commands::Source { command } => command.exec(&config).await?,
            Commands::Workflow => workflow::run(&config).await?,
            Commands::GithubTrending { output } => github_trending::run(&config, output).await?,
        }
        Ok(())
    }
}
