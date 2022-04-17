use anyhow::{Context, Result};
use chrono::prelude::*;
use chronoutil::RelativeDuration;
use tokio::net::UdpSocket;
use tracing::debug;

// parse a time string from english, return a unix timestamp
pub fn parse_time(s: &str) -> Result<i64> {
    let duration = chrono_english::parse_duration(s);
    if let Ok(duration) = duration {
        debug!("Parsed duration '{}' as {:#?}", s, duration);
        // note: making the duration always negative here so '5 min' and '5 min ago' both
        // resolve to a time in the past. in other contexts you care about the future, but
        // here we're querying history, so no good can come of parsing `history -s "5 min"`
        // as looking for commands that finished in the future
        let chronoduration = match duration {
            chrono_english::Interval::Seconds(s) => RelativeDuration::seconds((-s.abs()).into()),
            chrono_english::Interval::Days(d) => RelativeDuration::days((-d.abs()).into()),
            chrono_english::Interval::Months(m) => RelativeDuration::months(-m.abs()),
        };
        let then = Utc::now() + chronoduration;
        return Ok(then.timestamp());
    }

    let date = chrono_english::parse_date_string(s, Local::now(), chrono_english::Dialect::Us);
    match date {
        Ok(date) => {
            debug!("Parsed date '{}' as {:#?}", s, date);
            Ok(date.timestamp())
        }
        Err(_) => Err(duration.unwrap_err().into()),
    }
}

pub fn getshorthostname() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .split('.')
        .next()
        .unwrap()
        .to_string()
}

pub fn getsession() -> Result<i32> {
    ctty::get_path_for_dev(
        ctty::get_ctty_dev().context("Unable to get this processes controlling tty")?,
    )
    .context("Unable to get path to controlling tty")?
    .replace("/dev/pts/", "")
    .parse::<i32>()
    .context("Unable to parse controlling tty as an integer inside /dev/pts/")
}

pub async fn addr_routes_to_me(addr: &str) -> Result<bool> {
    let server = UdpSocket::bind("0.0.0.0:0").await?;
    let client = UdpSocket::bind("0.0.0.0:0").await?;
    if let Err(_) = client
        .connect(format!("{}:{}", addr, server.local_addr()?.port()))
        .await
    {
        return Ok(false);
    };
    let msg = b"DEADBEEF";
    client.send(msg).await?;
    let mut buf = vec![0u8; 128];
    // From my ping attempts, I get like 0.1 or 0.05ms to ping localhost
    let nbytes =
        match tokio::time::timeout(std::time::Duration::from_millis(25), server.recv(&mut buf))
            .await
        {
            Ok(r) => r?,
            Err(_) => {
                return Ok(false);
            }
        };
    Ok(&buf[..nbytes] == msg)
}

#[tokio::test]
async fn test_1() {
    assert_eq!(addr_routes_to_me("127.0.0.1").await.unwrap(), true);
    assert_eq!(
        addr_routes_to_me(&format!(
            "{}.local",
            gethostname::gethostname().to_string_lossy()
        ))
        .await
        .unwrap(),
        true
    );
    assert_eq!(addr_routes_to_me("foo").await.unwrap(), false);
    assert_eq!(addr_routes_to_me("google.com").await.unwrap(), false);
}
