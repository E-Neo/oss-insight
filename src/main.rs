use anyhow::Result;
use clap::Parser;

use crate::commands::Cli;

mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    cli.exec().await?;
    Ok(())
}
