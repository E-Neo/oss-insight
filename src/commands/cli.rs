use anyhow::Result;
use clap::{Parser, Subcommand};
use oss_insight_db::Db;

use crate::commands::{config::Config, source::SourceCommands, workflow};

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
}

impl Cli {
    pub async fn exec(&self) -> Result<()> {
        let config = Config::load()?;
        let _db = Db::open(&config.db_path()?).await?;
        match &self.command {
            Commands::Source { command } => command.exec(&config).await?,
            Commands::Workflow => workflow::run(&config).await?,
        }
        Ok(())
    }
}
