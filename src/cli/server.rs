use std::{process::exit, sync::Arc};

use crate::{
    monitor::server_monitor_log_forever, schema::create_schema, tcp::HistoryQueryServer,
    udp::InsertServer,
};
use anyhow::Result;
use clap::Parser;
use rusqlite::Connection;
use tokio::sync::Mutex;

use super::register_tracing;

#[derive(Parser, Debug)]
pub struct ServerOptions {
    /// Become a daemon
    #[clap(long)]
    daemonize: bool,

    /// History file (sqlite db)
    #[clap()]
    history: String,
}

pub fn server_main() -> Result<()> {
    let options = ServerOptions::parse();
    match options.daemonize {
        true => {
            let stdout = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("/tmp/history-daemon.log")?;
            let stderr = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("/tmp/history-daemon.log")?;
            let daemonize = daemonize::Daemonize::new()
                .working_directory("/tmp")
                .stdout(stdout)
                .stderr(stderr);
            match daemonize.start() {
                Ok(_) => server_main_impl(options, true),
                Err(e) => {
                    eprintln!("Error, {}", e);
                    Ok(())
                }
            }
        }
        false => server_main_impl(options, false),
    }
}

fn server_main_impl(options: ServerOptions, daemonized: bool) -> Result<()> {
    let _guard = register_tracing(daemonized)?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Unable to build tokio runtime");

    rt.block_on(async move {
        tracing::info!(
            "Booting history server on hostname={:} pid={} db={:}",
            *crate::MYHOSTNAME,
            std::process::id(),
            options.history,
        );
        let con = Connection::open(options.history)?;
        create_schema(&con)?;
        let con = Arc::new(Mutex::new(con));
        let udp_server = InsertServer::new(con.clone()).await?;
        let tcp_server = HistoryQueryServer::new(con.clone());

        let mon = tokio::spawn(async { server_monitor_log_forever().await });
        let udp = tokio::spawn(async move { udp_server.run().await });
        let tcp = tokio::spawn(async move { tcp_server.run().await });

        tokio::select! {
            r = mon => {
                r?
            }
            r = udp => {
                r?
            }
            r = tcp => {
                r?
            },
            _ = tokio::signal::ctrl_c() => {
                exit(1);
            }
        }?;
        Ok(())
    })
}
