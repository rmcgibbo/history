use anyhow::{Context, Result};
use crossterm::event::{KeyEvent, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{self, Clear};
use crossterm::{
    event::{read, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::fs::File;
use std::io::{stdout, BufWriter, Write};
use std::os::unix::io::FromRawFd;
use tarpc::tokio_serde::formats::Bincode;
use tarpc::{client, context};

use crate::tcp::HistoryQueryServiceClient;

static PROMPT: &str = "(reverse-i-search)";
static FAILED_PROMPT: &str = "(failed reverse-i-search)";

async fn main_loop(client: HistoryQueryServiceClient) -> Result<()> {
    let mut stdout = stdout();
    let mut term_dimensions = crossterm::terminal::size()?;
    let mut fd3 = BufWriter::new(unsafe { File::from_raw_fd(3) });
    let mut last_n_term_chars_printed = 0;

    let mut query = String::new();
    let mut last_match: Option<String> = None;
    let mut offset_from_end: u32 = 0;
    write!(stdout, "{}`': ", PROMPT).unwrap();
    stdout.flush().unwrap();

    loop {
        // Blocking read
        match read()? {
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
            }) => {
                crossterm::execute!(
                    stdout,
                    crossterm::cursor::MoveToPreviousLine(
                        last_n_term_chars_printed / (term_dimensions.0 + 1),
                    ),
                    Print("\r"),
                    Clear(terminal::ClearType::FromCursorDown),
                )
                .unwrap();
                write!(fd3, "n {}", &last_match.unwrap_or("".to_string())).unwrap();
                break;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
            }) => {
                crossterm::execute!(
                    stdout,
                    crossterm::cursor::MoveToPreviousLine(
                        last_n_term_chars_printed / (term_dimensions.0 + 1),
                    ),
                    Print("\r"),
                    Clear(terminal::ClearType::FromCursorDown),
                )
                .unwrap();
                write!(fd3, "a {}", &last_match.unwrap_or("".to_string())).unwrap();
                break;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::CONTROL,
            }) => {
                crossterm::execute!(
                    stdout,
                    crossterm::cursor::MoveToPreviousLine(
                        last_n_term_chars_printed / (term_dimensions.0 + 1),
                    ),
                    Print("\r"),
                    Clear(terminal::ClearType::FromCursorDown),
                )
                .unwrap();
                write!(fd3, "_ {}", &last_match.unwrap_or("".to_string())).unwrap();
                break;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::CONTROL,
            }) => {
                offset_from_end += 1;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('s'),
                modifiers: KeyModifiers::CONTROL,
            }) => {
                offset_from_end = match offset_from_end {
                    0 => 0,
                    x => x - 1,
                };
            }
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
            }) => {
                offset_from_end = 0;
                query.pop();
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
            }) => {
                crossterm::execute!(stdout, Clear(terminal::ClearType::CurrentLine), Print("\r"))
                    .unwrap();
                break;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::SHIFT,
            }) => {
                offset_from_end = 0;
                query.push(c);
            }
            Event::Resize(x, y) => {
                term_dimensions = (x, y);
            }

            _ => {
                // println!("{:#?}", x);
            }
        }
        let q = crate::tcp::IsearchQuery {
            command: query.clone(),
            limit: 1,
            dir: crate::CWD.to_string(),
            offset: offset_from_end,
        };
        //eprintln!("{:#?}", q);
        let result = client.isearch(context::current(), q).await??;
        match result.get(0).map(|x| x.argv.clone()) {
            Some(c) => {
                crossterm::execute!(
                    stdout,
                    crossterm::cursor::MoveToPreviousLine(
                        last_n_term_chars_printed / (term_dimensions.0 + 1),
                    ),
                    Print("\r"),
                    Clear(terminal::ClearType::FromCursorDown),
                    Print(PROMPT),
                    Print("`"),
                    Print(&query),
                    Print("': "),
                    Print(highlight(&c, &query))
                )?;
                last_n_term_chars_printed = (PROMPT.len() + query.len() + 4 + c.len()) as u16;
                last_match = Some(c);
            }
            None => {
                crossterm::execute!(
                    stdout,
                    crossterm::cursor::MoveToPreviousLine(
                        last_n_term_chars_printed / (term_dimensions.0 + 1)
                    ),
                    Print("\r"),
                    Clear(terminal::ClearType::FromCursorDown),
                    Print(FAILED_PROMPT),
                    Print("`"),
                    Print(&query),
                    Print("': "),
                    Print(
                        last_match
                            .as_ref()
                            .map(|x| highlight(x, &query))
                            .unwrap_or("".to_string())
                    )
                )?;
                last_n_term_chars_printed = (FAILED_PROMPT.len()
                    + query.len()
                    + 4
                    + last_match.as_ref().map(|x| x.len()).unwrap_or(0))
                    as u16;
            }
        }
    }

    Ok(())
}

fn highlight(result: &str, query: &str) -> String {
    result.replace(
        query,
        &format!(
            "{}{}{}",
            crossterm::style::Attribute::Reverse,
            query,
            crossterm::style::Attribute::Reset,
        ),
    )
}

pub async fn isearch_main() -> Result<()> {
    let server = crate::HISTORY_SERVER
        .as_ref()
        .context("Unable to access environment variable '__history_server'")
        .context("Did you forget to 'eval \"$(history --eval <server-name>)\"' in your .bashrc?")?;
    let transport = tarpc::serde_transport::tcp::connect(
        format!("{}:{}", server, crate::HISTORY_PORT),
        Bincode::default,
    )
    .await?;

    let client = HistoryQueryServiceClient::new(client::Config::default(), transport).spawn();

    if let Ok(q) = std::env::var("__history_query_debug") {
        let q = crate::tcp::IsearchQuery {
            command: q,
            limit: 10,
            dir: crate::CWD.to_string(),
            offset: 0,
        };
        eprintln!("{:#?}", q);
        let result = client.isearch(context::current(), q).await??;
        println!("result={:#?}", result);
        std::process::exit(1);
    }

    enable_raw_mode()?;
    crossterm::execute!(stdout(), crossterm::cursor::Hide)?;

    if let Err(e) = main_loop(client).await {
        println!("Error: {:?}\r", e);
    }

    crossterm::execute!(stdout(), crossterm::cursor::Show)?;
    Ok(disable_raw_mode()?)
}
