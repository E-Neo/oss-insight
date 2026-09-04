use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::source::SourceCommands;

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
        match &self.command {
            Commands::Source { command } => command.exec().await?,
        }
        Ok(())
    }
}
