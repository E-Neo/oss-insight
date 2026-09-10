use std::io;

use anyhow::Result;
use clap::Parser;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

use crate::commands::Cli;
use crate::commands::Config;

mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = init_logging()?;
    let cli = Cli::parse();
    cli.exec().await?;
    Ok(())
}

fn init_logging() -> Result<WorkerGuard> {
    let log_dir = Config::log_dir();
    std::fs::create_dir_all(&log_dir)?;
    let appender = tracing_appender::rolling::daily(&log_dir, "oss-insight.log");
    let (file, guard) = tracing_appender::non_blocking(appender);

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(io::stderr)
                .with_filter(EnvFilter::from_default_env()),
        )
        .with(
            fmt::layer()
                .with_writer(file)
                .with_ansi(false)
                .with_target(false)
                .with_filter(LevelFilter::DEBUG),
        )
        .init();

    Ok(guard)
}
