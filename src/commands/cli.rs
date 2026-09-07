use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::{config::Config, source::SourceCommands};

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
}

impl Cli {
    pub async fn exec(&self) -> Result<()> {
        let config = Config::load()?;
        match &self.command {
            Commands::Source { command } => command.exec(&config).await?,
        }
        Ok(())
    }
}
