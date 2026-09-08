//! The terminal device itself, independent of the inherited streams.

use std::fs::{File, OpenOptions};
use std::io::Write;

use anyhow::{Context, Result};

/// Opens the terminal this process is attached to.
///
/// Neither inherited stream is safe to write to. stdout belongs to the shell
/// integration, which reads the chosen command from it, and a PSReadLine key
/// handler hands the child a redirected stderr, so anything written there
/// disappears into a pipe.
pub fn device() -> Result<File> {
    let path = if cfg!(windows) { "CONOUT$" } else { "/dev/tty" };

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to open the terminal device ({path})"))
}

/// Puts a message in front of the user, wherever it can still be seen.
///
/// Falls back to stderr, which is better than nothing when there is no terminal
/// at all, such as under a test harness or a redirect.
pub fn report(message: &str) {
    if let Ok(mut device) = device()
        && device.write_all(message.as_bytes()).is_ok()
    {
        return;
    }

    eprint!("{message}");
}
