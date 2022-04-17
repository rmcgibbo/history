use anyhow::{anyhow, Context, Result};
use rusqlite::params;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;

const MAX_DATAGRAM_SIZE: usize = 65_507;

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcMessage {
    pub host: String,
    pub session: i32,
    pub exit_status: i32,
    pub dir: String,
    pub argv: String,
    pub time: u64,
}

pub struct InsertServer {
    socket: UdpSocket,
    buf: Vec<u8>,
    con: Arc<Mutex<Connection>>,
}

impl InsertServer {
    pub async fn new(con: Arc<Mutex<rusqlite::Connection>>) -> Result<InsertServer> {
        let addr = format!("0.0.0.0:{}", crate::HISTORY_PORT);
        info!("Lisening on {}", addr);
        let socket = UdpSocket::bind(&addr).await?;
        Ok(InsertServer {
            socket,
            buf: vec![0; MAX_DATAGRAM_SIZE],
            con,
        })
    }
    pub async fn run(self) -> Result<()> {
        let InsertServer {
            socket,
            mut buf,
            con,
        } = self;

        loop {
            if let Err(e) = InsertServer::run_one(&con, &socket, &mut buf).await {
                error!("{:#}", e);
            }
        }
    }
    async fn run_one(
        con: &Arc<Mutex<Connection>>,
        socket: &UdpSocket,
        buf: &mut Vec<u8>,
    ) -> Result<()> {
        let nbytes = socket
            .recv(buf)
            .await
            .context("Receiving bytes from socket")?;
        let msg = deserialize(&buf[..nbytes])?;
        insert(&*con.lock().await, &msg).context("Inserting into history database")?;
        Ok(())
    }
}

fn deserialize(buf: &[u8]) -> Result<RpcMessage> {
    let ctx = || {
        format!(
            "Failure to parse UDP datagram {:#?}",
            String::from_utf8_lossy(buf)
        )
    };

    let fields: Vec<&[u8]> = buf.split(|&c| c == b'\0').collect();
    match &fields[..] {
        [v_session, v_hostname, v_exit_status, v_pwd, v_argv_with_line_number] => {
            let session = String::from_utf8_lossy(v_session)
                .parse::<i32>()
                .with_context(|| {
                    format!(
                        "Unable to parse session id (first field) {:#?} as i32",
                        String::from_utf8_lossy(v_session)
                    )
                })
                .with_context(ctx)?;
            let host = String::from_utf8_lossy(v_hostname).to_string();
            let exit_status = String::from_utf8_lossy(v_exit_status)
                .parse::<i32>()
                .with_context(|| {
                    format!(
                        "Unable to parse exit status (third field) {:#?} as i32",
                        String::from_utf8_lossy(v_exit_status)
                    )
                })
                .with_context(ctx)?;
            let dir = String::from_utf8_lossy(v_pwd).to_string();
            let v_argv_without_line_number = v_argv_with_line_number.get(7..);
            let v_argv = v_argv_without_line_number.ok_or_else(|| anyhow!("The command line (third field), ostensibly from $(history 1) is too short, and doesn't contain the expected leading line number"))
            .with_context(ctx)?;
            Ok(RpcMessage {
                session,
                host,
                exit_status,
                dir,
                argv: String::from_utf8_lossy(v_argv).to_string(),
                time: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs(),
            })
        }
        _ => {
            anyhow::bail!(
                "Unable to parse UDP datagram {:#?} as 5 null-separated fields",
                String::from_utf8_lossy(buf)
            );
        }
    }
}

fn insert(con: &rusqlite::Connection, msg: &RpcMessage) -> Result<()> {
    let command_id = match con
        .prepare("insert into commands (argv) values (?)")?
        .insert(params![msg.argv])
    {
        Ok(i) => i,
        Err(_) => con
            .prepare("select id from commands where argv = ?")?
            .query_row(params![msg.argv], |row| row.get(0))?,
    };
    let place_id = match con
        .prepare("insert into places (host, dir) values (?, ?)")?
        .insert(params![msg.host, msg.dir])
    {
        Ok(i) => i,
        Err(_) => con
            .prepare("select id from places where host = ? AND dir = ?")?
            .query_row(params![msg.host, msg.dir], |row| row.get(0))?,
    };
    con.execute(
        "insert into history (session, command_id, place_id, exit_status, end_time)
                                  values (?, ?, ?, ?, ?)",
        params![msg.session, command_id, place_id, msg.exit_status, msg.time],
    )?;

    Ok(())
}
