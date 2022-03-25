use std::env::VarError;

pub mod cli;
mod monitor;
mod schema;
mod tcp;
mod udp;
mod util;

pub const HISTDB_PORT: u16 = 29080;
lazy_static::lazy_static! {
    static ref HISTDB_SERVER: Result<String, VarError> = std::env::var("__histdb_server");
    static ref MYHOSTNAME: String = gethostname::gethostname().to_str().unwrap().to_string();
    static ref CWD: String = std::env::current_dir().unwrap().display().to_string();
    pub static ref HISTDB_MODE: Result<String, VarError> = std::env::var("__histdb_mode");
}
