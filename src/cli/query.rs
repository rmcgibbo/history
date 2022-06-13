use anyhow::{Context, Result};
use chrono::prelude::*;
use git_version::git_version;
use clap::{AppSettings, Parser};
use stybulate::{Cell, Headers, Style, Table};
use tarpc::{client, context, tokio_serde::formats::Bincode};

use crate::tcp::HistoryQueryServiceClient;

/// Search shell command history
#[derive(Parser, Debug)]
#[clap(author, version = git_version!(fallback="0.1"), about, long_about = None)]
#[clap(global_setting(AppSettings::DeriveDisplayOrder))]
pub struct QueryClientOptions {
    /// Show only N rows
    #[clap(value_name = "N", short = 'n', long = "--limit", default_value = "25")]
    limit: i32,

    /// Show only entries from session T.
    #[clap(value_name = "T", short = 't', long = "--tty")]
    session: Option<Option<i32>>,

    /// Show only entries since the specified date.
    #[clap(value_name = "TIME", short = 's', long)]
    since: Option<String>,

    /// Show only commands before date.
    #[clap(value_name = "TIME", short = 'u', long)]
    until: Option<String>,

    /// Show only rows with exit status X. Can be 'error' to find all nonzero.
    #[clap(value_name = "X", short = 'x', long)]
    status: Option<Option<String>>,

    /// Reverse sort order of results.
    #[clap(long = "--desc")]
    desc: bool,

    /// Find only entries run in the current dir or below if no DIR, or
    /// find only entries in directory <DIR> or below.
    #[clap(value_name = "[DIR]", long = "--in")]
    indir: Option<Option<String>>,

    /// Like --in, but excluding subdirectories.
    #[clap(value_name = "[DIR]", long = "--at")]
    at: Option<Option<String>>,

    /// Print the host column and show all hosts if no HOSTNAME
    /// or find only entries from host HOSTNAME.
    #[clap(value_name = "[HOSTNAME]", long)]
    host: Option<Option<String>>,

    /// Don't match substrings in <command>.
    #[clap(long = "--exact")]
    exact: bool,

    /// Don't print header.
    #[clap(long = "--no-header")]
    nh: bool,

    /// Generate eval string for bash (use eval "$(history --eval <ADDR>)"). Supply server addr,
    /// like 127.0.0.1 if you want to run the server locally, or remote addr/ip if you want to
    /// centralize the history.
    #[clap(long = "--eval", name = "SERVER_ADDR")]
    eval: Option<String>,

    /// Search history for commands containing this fragment.
    #[clap()]
    command: Option<String>,
}

pub async fn query_client_main() -> Result<()> {
    let options = QueryClientOptions::parse();

    if let Some(server_addr) = options.eval {
        let shell = std::env::var("SHELL")
            .context("Unable to read environment variable SHELL")
            .context("Sorry, history only supports the bash shell.")?;
        if !shell.ends_with("bash") {
            anyhow::bail!("Sorry, history only supports the bash shell. I see from $SHELL you're running from {:?}", shell);
        }
        return crate::cli::eval::show_bash_eval_string(server_addr).await;
    }
    let server = crate::HISTORY_SERVER
        .as_ref()
        .context("Unable to access environment variable '__history_server'")
        .context("Did you forget to 'eval \"$(history --eval <server-name>)\"' in your .bashrc?")?;

    let transport = tarpc::serde_transport::tcp::connect(
        format!("{}:{}", server, crate::HISTORY_PORT),
        Bincode::default,
    )
    .await?;

    let now = Utc::now();
    let client = HistoryQueryServiceClient::new(client::Config::default(), transport).spawn();
    let mysession = crate::util::getsession().context("Unable to get current tty session")?;
    let parse_time = |x: Option<&String>| -> Result<Option<i64>> {
        match x {
            Some(s) => Ok(Some(crate::util::parse_time(s)?)),
            None => Ok(None),
        }
    };
    let display_host_column = options.host == Some(None);
    let display_tty_column = options.session.is_none();
    let display_dir_column = options.at.is_none();

    let query = crate::tcp::Query {
        // options.host == None => restrict query to this host
        // options.host == Some(None) => all hosts
        // options.host == Some(Some(s)) ==> restrict query to host s
        host: match options.host {
            None => Some(crate::MYHOSTNAME.clone()),
            Some(None) => None,
            Some(Some(s)) => Some(s),
        },
        command: options.command,
        exact: options.exact,
        indir: options
            .indir
            .map(|x| x.unwrap_or_else(|| crate::CWD.to_string())),
        atdir: options
            .at
            .map(|x| x.unwrap_or_else(|| crate::CWD.to_string())),
        session: options.session.map(|x| x.unwrap_or(mysession)),
        status: options.status.map(|x| x.unwrap_or("error".to_string())),
        since: parse_time(options.since.as_ref())?,
        until: parse_time(options.until.as_ref())?,
        desc: options.desc,
        limit: options.limit,
    };
    tracing::debug!("{:#?}", query);

    let out: Vec<Vec<Cell>> = client
        .query(context::current(), query)
        .await??
        .into_iter()
        .map(|row| {
            let dt = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(row.time, 0), Utc);
            let local = DateTime::<Local>::from(dt);
            let date = if dt.date() == now.date() {
                Cell::from(&local.format("%-I:%M%p").to_string())
            } else {
                Cell::from(&local.format("%m/%d").to_string())
            };
            let mut fmtrow = vec![date];
            if display_host_column {
                fmtrow.push(Cell::from(&remove_zero_width_graphemes(&row.host)));
            }
            if display_tty_column {
                fmtrow.push(Cell::Int(row.session));
            }
            if display_dir_column {
                fmtrow.push(Cell::from(&remove_zero_width_graphemes(&row.dir)));
            }

            fmtrow.push(Cell::from(&remove_zero_width_graphemes(&row.argv)));
            fmtrow
        })
        .collect();

    let result = Table::new(
        Style::Plain,
        out,
        if options.nh {
            None
        } else {
            let mut keys = vec!["time"];
            if display_host_column {
                keys.push("host");
            }
            if display_tty_column {
                keys.push("tty");
            }
            if display_dir_column {
                keys.push("dir");
            }
            keys.push("cmd");

            Some(Headers::from(keys))
        },
    )
    .tabulate();
    println!("{}", result);
    Ok(())
}

// Fix for https://github.com/guigui64/stybulate/issues/18
fn remove_zero_width_graphemes(s: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;

    UnicodeSegmentation::graphemes(s, true)
        .map(|x| match UnicodeWidthStr::width(x) {
            0 => "",
            _ => x,
        })
        .collect()
}
