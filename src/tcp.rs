use anyhow::Result;
use futures_util::StreamExt;
use rusqlite::types::ToSqlOutput;
use rusqlite::ToSql;
use rusqlite::{named_params, params_from_iter};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tarpc::{
    context,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Bincode,
};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error};

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum RpcError {
    #[error("Invalid filename: {path}")]
    InvalidFilename { path: std::path::PathBuf },

    #[error("IoError: {msg}")]
    IoError { msg: String },

    #[error("SqlError: {msg}")]
    SqlError { msg: String },

    #[error("OtherError: {msg}")]
    OtherError { msg: String },
}

impl From<std::io::Error> for RpcError {
    fn from(e: std::io::Error) -> RpcError {
        RpcError::IoError {
            msg: format!("{}", e),
        }
    }
}

impl From<rusqlite::Error> for RpcError {
    fn from(e: rusqlite::Error) -> RpcError {
        RpcError::SqlError {
            msg: format!("{}", e),
        }
    }
}

impl From<anyhow::Error> for RpcError {
    fn from(e: anyhow::Error) -> RpcError {
        RpcError::OtherError {
            msg: format!("{}", e),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Query {
    pub host: Option<String>,
    pub command: Option<String>,
    pub exact: bool,
    pub indir: Option<String>,
    pub atdir: Option<String>,
    pub session: Option<i32>,
    pub status: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub desc: bool,
    pub limit: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IsearchQuery {
    pub command: String,
    pub dir: String,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QueryResultRow {
    pub time: i64,
    pub session: i32,
    pub argv: String,
    pub dir: String,
    pub host: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SqlType {
    Int(i64),
    String(String),
}

#[tarpc::service]
pub trait HistdbQueryService {
    async fn query(query: Query) -> core::result::Result<Vec<QueryResultRow>, RpcError>;
    async fn isearch(query: IsearchQuery) -> core::result::Result<Vec<QueryResultRow>, RpcError>;
}

#[derive(Clone, Debug)]
struct HistdbQueryServerImpl {
    con: Arc<Mutex<rusqlite::Connection>>,
}

#[tarpc::server]
impl HistdbQueryService for HistdbQueryServerImpl {
    async fn isearch(
        self,
        _ctx: context::Context,
        query: IsearchQuery,
    ) -> core::result::Result<Vec<QueryResultRow>, RpcError> {
        let q = r#"
        SELECT argv
        FROM history
        JOIN commands on history.command_id = commands.id
        JOIN places on history.place_id = places.id
        WHERE argv LIKE ('%' || :argv || '%') ESCAPE '\'
        GROUP BY history.command_id, history.place_id
        ORDER BY
            max(history.id) DESC,
            argv LIKE (:argv || '%') DESC,
            dir LIKE (:dir || '%') DESC
        LIMIT :limit
        OFFSET :offset;
        "#;
        let like_escape = |s: &str| ToSqlOutput::from(s.replace("%", "\\%").replace("_", "\\_"));

        let con = self.con.lock().await;
        let params = named_params! {
            ":argv": like_escape(&query.command),
            ":dir": like_escape(&query.dir),
            ":limit": query.limit.to_sql()?,
            ":offset": query.offset.to_sql()?,
        };

        let mut stmt = con.prepare(&q)?;
        let mut rows = stmt.query(params)?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(QueryResultRow {
                argv: row.get(0)?,
                time: 0,
                session: 0,
                dir: "".to_string(),
                host: "".to_string(),
            });
        }

        Ok(result)
    }

    async fn query(
        self,
        _ctx: context::Context,
        query: Query,
    ) -> core::result::Result<Vec<QueryResultRow>, RpcError> {
        let Query {
            host,
            command,
            exact,
            indir,
            atdir,
            session,
            status,
            since,
            until,
            desc,
            limit,
        } = query;

        debug!("Received query");
        let (hostwhere, hostwhereparams) = match host.as_ref() {
            Some(h) => ("places.host = ?", Some(h.to_sql()?)),
            None => ("1", None),
        };
        let (commandwhere, commandwhereparams) = match (command.as_ref(), exact) {
            (Some(cmd), false) => (
                "commands.argv GLOB ?",
                Some(ToSqlOutput::from(format!("*{}*", cmd))),
            ),
            (Some(cmd), true) => ("commands.argv = ?", Some(cmd.to_sql()?)),
            _ => ("1", None),
        };
        let (indirwhere, indirwhereparams) = match indir.as_ref() {
            Some(indir) => (
                "places.dir LIKE ?",
                Some(ToSqlOutput::from(format!("{}%", indir))),
            ),
            None => ("1", None),
        };
        let (atdirwhere, atdirwhereparams) = match atdir.as_ref() {
            Some(atdir) => ("places.dir = ?", Some(atdir.to_sql()?)),
            None => ("1", None),
        };
        let (sessionwhere, sessionwhereparams) = match session.as_ref() {
            Some(session) => ("session = ?", Some(session.to_sql()?)),
            None => ("1", None),
        };
        let (statuswhere, statuswhereparams) = match status.as_ref() {
            Some(x) if x == "error" => ("history.exit_status > 0", None),
            Some(x) => ("cast(history.exit_status as str) = ?", Some(x.to_sql()?)),
            None => ("1", None),
        };
        let (sincewhere, sincewhereparams) = match since.as_ref() {
            Some(x) => ("history.end_time >= ?", Some(x.to_sql()?)),
            None => ("1", None),
        };
        let (untilwhere, untilwhereparams) = match until.as_ref() {
            Some(x) => ("history.end_time <= ?", Some(x.to_sql()?)),
            None => ("1", None),
        };
        let query = format!(
            "
            SELECT end_time, session, argv, dir, host, max(end_time) as max_time
            FROM commands
            JOIN history on history.command_id = commands.id
            JOIN places on history.place_id = places.id
            WHERE {hostwhere}
              AND {commandwhere}
              AND {indirwhere}
              AND {atdirwhere}
              AND {sessionwhere}
              AND {statuswhere}
              AND {sincewhere}
              AND {untilwhere}
            GROUP BY history.command_id, history.place_id
            ORDER BY max_time DESC
            LIMIT {limit}
        "
        );
        let paramv = vec![
            hostwhereparams,
            commandwhereparams,
            indirwhereparams,
            atdirwhereparams,
            sessionwhereparams,
            statuswhereparams,
            sincewhereparams,
            untilwhereparams,
        ]
        .into_iter()
        .flatten();
        let params = params_from_iter(paramv);
        let con = self.con.lock().await;
        let mut stmt = con.prepare(&query)?;
        let mut rows = stmt.query(params)?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(QueryResultRow {
                time: row.get(0)?,
                session: row.get(1)?,
                argv: row.get(2)?,
                dir: row.get(3)?,
                host: row.get(4)?,
            });
        }

        if !desc {
            result.reverse();
        }

        debug!("Returned response. {} rows", result.len());
        Ok(result)
    }
}
pub struct HistdbQueryServer {
    con: Arc<Mutex<rusqlite::Connection>>,
}
impl HistdbQueryServer {
    pub fn new(con: Arc<Mutex<rusqlite::Connection>>) -> HistdbQueryServer {
        HistdbQueryServer { con }
    }
    pub async fn run(self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", crate::HISTDB_PORT);
        let mut incoming = tarpc::serde_transport::tcp::listen(&addr, Bincode::default).await?;
        loop {
            if let Some(x) = incoming.next().await {
                match x {
                    Ok(transport) => {
                        let server = HistdbQueryServerImpl {
                            con: self.con.clone(),
                        };
                        let fut = BaseChannel::with_defaults(transport).execute(server.serve());
                        tokio::spawn(fut);
                    }
                    Err(e) => {
                        error!("{}", e)
                    }
                };
            }
        }
    }
}
