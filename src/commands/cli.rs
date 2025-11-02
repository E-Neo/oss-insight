use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::crawler::CrawlerCommands;

#[derive(Parser)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Information crawler.
    Crawler {
        #[command(subcommand)]
        command: CrawlerCommands,
    },
}

impl Cli {
    pub async fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Crawler { command } => command.exec().await?,
        }
        Ok(())
    }
}
