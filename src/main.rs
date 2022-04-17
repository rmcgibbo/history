use anyhow::Result;
use history::cli::register_tracing;
use history::cli::{isearch_main, query_client_main, server_main};

fn main() -> Result<()> {
    let rt = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Unable to construct tokio runtime")
    };

    match history::HISTORY_MODE.as_ref().map(|x| x as &str) {
        Ok("server") => server_main(), // tracing is registered later
        Ok("isearch") => Ok(rt().block_on(async { isearch_main().await })?),
        _ => {
            register_tracing(false)?;
            Ok(rt().block_on(async { query_client_main().await })?)
        }
    }
}
