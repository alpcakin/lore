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

/// Puts the controlling terminal on stdin when something else is there.
///
/// zsh runs a widget's commands with stdin on `/dev/null`. The terminal library
/// then falls back to opening `/dev/tty`, and on macOS the kernel refuses to
/// poll that device: the answer to the cursor position question is never seen,
/// and the picker waits for it forever. The pseudo terminal's own path, such as
/// `/dev/ttys003`, has no such problem, so it is found by session and dup'd
/// onto stdin before the terminal library looks. Nothing here reads stdin for
/// anything else.
///
/// Best effort: when the terminal cannot be found, the picker proceeds as it
/// would have and whatever happens next is reported the usual way.
#[cfg(target_os = "macos")]
pub fn adopt_terminal_as_stdin() {
    use std::os::fd::AsRawFd;

    // SAFETY: isatty only inspects a descriptor number.
    if unsafe { libc::isatty(libc::STDIN_FILENO) } == 1 {
        return;
    }

    let Some(terminal) = controlling_terminal() else {
        return;
    };

    // SAFETY: both descriptors are open and owned by this process; dup2 leaves
    // `terminal` untouched, and the copy on stdin outlives it.
    unsafe {
        libc::dup2(terminal.as_raw_fd(), libc::STDIN_FILENO);
    }
}

/// The pseudo terminal this process's session is attached to, if any.
///
/// Only `ttys*` devices are tried. Anything else under `/dev/tty*` is a
/// serial or Bluetooth port, and opening one of those blocks waiting for a
/// carrier that will never come.
#[cfg(target_os = "macos")]
fn controlling_terminal() -> Option<File> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    // SAFETY: getsid(0) asks about the calling process and cannot fail for it.
    let session = unsafe { libc::getsid(0) };

    std::fs::read_dir("/dev")
        .ok()?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("ttys"))
        .find_map(|entry| {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY)
                .open(entry.path())
                .ok()?;

            // SAFETY: the descriptor is open and owned by `file`.
            let owner = unsafe { libc::tcgetsid(file.as_raw_fd()) };
            (owner == session).then_some(file)
        })
}

#[cfg(not(target_os = "macos"))]
pub fn adopt_terminal_as_stdin() {}
