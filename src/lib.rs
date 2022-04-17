use std::env::VarError;

pub mod cli;
mod monitor;
mod schema;
mod tcp;
mod udp;
mod util;

pub const HISTORY_PORT: u16 = 29080;
lazy_static::lazy_static! {
    static ref HISTORY_SERVER: Result<String, VarError> = std::env::var("__history_server");
    static ref MYHOSTNAME: String = util::getshorthostname();
    static ref CWD: String = std::env::var("__history_pwd").unwrap_or_else(|_| std::env::current_dir().unwrap().display().to_string());
    pub static ref HISTORY_MODE: Result<String, VarError> = std::env::var("__history_mode");
}
