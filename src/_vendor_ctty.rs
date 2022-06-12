//! Ctty-rs is a cross-platform crate for determining a processes' controlling TTY (ctty).
//! Support is currently available for Linux, macOS, and FreeBSD.
//!
//! In many cases, it may be useful to know which TTY a process belongs to
//! (for example, when storing session data), but there is no standardized way to
//! do this across operating systems. One way is to use ttyname on stdin, stout, or stderr's
//! file descriptors, but this doesn't work in cases where they are redirected at the shell level.
//!
//! ctty-rs provides a simple way to obtain a processes' controlling TTY even when
//! stdin, stdout, and stderr with a platform-agnostic interface.

//
// Copyright 2017 Shawn Anastasio
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CttyError {
    #[error("Controlling TTY for this process not found")]
    NotFound,

    #[error("System returned invalid data when looking up CTTY")]
    SystemDataParseFailure,

    // #[error("Failed to request CTTY information from system")]
    // SystemPermissionFailure,
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

use std::fs::File;
use std::io::prelude::*;

use glob::glob;
use nix::sys::stat::stat;

/// Returns the dev_t corresponding to the current process's controlling tty
pub fn get_ctty_dev() -> Result<u64, CttyError> {
    // /proc/self/stat contains the ctty's device id in field 7
    // Open it and read its contents to a string
    let mut stat_f = File::open("/proc/self/stat")?;
    let mut stat = String::new();
    stat_f.read_to_string(&mut stat)?;

    // Start looking at the string two positions after the last ')'
    // This is because the data inside the () may contain spaces
    let mut start_idx = stat.rfind(')').unwrap_or(0);
    if start_idx == 0 {
        return Err(CttyError::SystemDataParseFailure);
    }
    start_idx += 2;

    // Split by whitespace into array to easily access indices
    let values_str = &stat[start_idx..];
    let mut values = values_str.split_whitespace();

    // Extract 5th field from start (represented as i32)
    let dev = values.nth(4).ok_or(CttyError::SystemDataParseFailure)?;
    let dev_int = dev
        .parse::<i32>()
        .map_err(|_| CttyError::SystemDataParseFailure)?;

    // Cast result to u64 and return
    Ok(dev_int as u64)
}

/// Returns a full path to a tty or pseudo tty that corresponds with the given dev_t
pub fn get_path_for_dev(dev: u64) -> Result<String, CttyError> {
    // Check all devices in /dev/pts/* and /dev/tty* for a match
    let patterns = ["/dev/pts/*", "/dev/tty"];

    for i in 0..patterns.len() {
        for entry in glob(patterns[i]).unwrap() {
            let path = match entry {
                Ok(p) => p,
                Err(_) => {
                    // Silently continue
                    continue;
                }
            };

            // See if this device matches the request
            let stat = match stat(&path) {
                Ok(s) => s,
                Err(_) => {
                    // Silently continue
                    continue;
                }
            };

            if dev == stat.st_rdev {
                // Found device, return it
                return Ok(String::from(path.to_str().unwrap()));
            }
        }
    }

    Err(CttyError::NotFound)
}

#[cfg(test)]
mod tests {
    use crate::_vendor_ctty::{get_path_for_dev, get_ctty_dev};
    use std::error::Error;

    #[test]
    fn test_get_ctty_dev() -> Result<(), Box<dyn Error>> {
        let dev = get_ctty_dev().unwrap();
        dbg!(dev);
        let path = get_path_for_dev(dev)?;
        dbg!(path);
        Ok(())
    }
}
