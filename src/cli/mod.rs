use anyhow::Result;
mod eval;
mod isearch;
mod query;
mod server;
use tracing_appender::non_blocking::WorkerGuard;

pub fn register_tracing(daemonized: bool) -> Result<Option<WorkerGuard>> {
    if daemonized {
        let file_appender =
            tracing_appender::rolling::daily(std::env::var("HOME")?, ".history.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        tracing_subscriber::fmt()
            .with_writer(non_blocking)
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_env(
                    tracing_subscriber::EnvFilter::DEFAULT_ENV,
                )
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("history=info")),
            )
            .init();
        return Ok(Some(guard));
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env(tracing_subscriber::EnvFilter::DEFAULT_ENV)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("history=info")),
        )
        .init();

    Ok(None)
}

pub use isearch::*;
pub use query::*;
pub use server::*;
