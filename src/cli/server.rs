use std::{process::exit, sync::Arc};

use crate::{
    monitor::server_monitor_log_forever, schema::create_schema, tcp::HistdbQueryServer,
    udp::InsertServer,
};
use anyhow::Result;
use rusqlite::Connection;
use structopt::StructOpt;
use tokio::sync::Mutex;

use super::register_tracing;

#[derive(StructOpt, Debug)]
#[structopt(setting = structopt::clap::AppSettings::ColoredHelp)]
pub struct ServerOptions {
    /// Become a daemon
    #[structopt(long)]
    daemonize: bool,

    /// History file (sqlite db)
    #[structopt()]
    histdb: String,
}

pub fn server_main() -> Result<()> {
    let options = ServerOptions::from_args();
    match options.daemonize {
        true => {
            let stdout = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("/tmp/histdb-daemon.log")?;
            let stderr = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("/tmp/histdb-daemon.log")?;
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
            "Booting histdb server on hostname={:} pid={} db={:}",
            gethostname::gethostname().to_string_lossy(),
            std::process::id(),
            options.histdb,
        );
        let con = Connection::open(options.histdb)?;
        create_schema(&con)?;
        let con = Arc::new(Mutex::new(con));
        let udp_server = InsertServer::new(con.clone()).await?;
        let tcp_server = HistdbQueryServer::new(con.clone());

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
