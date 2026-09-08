//! The picker: an alternate screen on the terminal device, driven by key events.

mod app;
mod form;
mod view;

pub use app::{App, Outcome};

use std::fs::{File, OpenOptions};
use std::panic;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

type Screen = Terminal<CrosstermBackend<File>>;

/// Runs the picker and returns what the shell should do.
pub fn run(mut app: App) -> Result<Outcome> {
    let mut screen = enter()?;
    let outcome = event_loop(&mut screen, &mut app);
    leave(&mut screen)?;
    outcome
}

/// Opens the terminal device itself rather than drawing on an inherited stream.
///
/// stdout already belongs to the shell integration, which captures the chosen
/// command from it. stderr is not a safe alternative either: a PSReadLine key
/// handler hands the child process a redirected stderr, so drawing there goes
/// into a pipe and the picker stays invisible while still reading keys.
fn terminal_device() -> Result<File> {
    let path = if cfg!(windows) { "CONOUT$" } else { "/dev/tty" };

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to open the terminal device ({path})"))
}

fn event_loop(screen: &mut Screen, app: &mut App) -> Result<Outcome> {
    loop {
        screen.draw(|frame| view::draw(app, frame))?;

        // Windows reports key releases as well as presses; acting on both would
        // process every keystroke twice.
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(outcome) = app.on_key(key)?
        {
            return Ok(outcome);
        }
    }
}

fn enter() -> Result<Screen> {
    install_panic_hook();
    enable_raw_mode().context("failed to put the terminal into raw mode")?;

    let mut device = terminal_device()?;
    execute!(device, EnterAlternateScreen).context("failed to open the alternate screen")?;

    Terminal::new(CrosstermBackend::new(device)).context("failed to start the terminal backend")
}

fn leave(screen: &mut Screen) -> Result<()> {
    drain_input();
    restore();
    screen.show_cursor().ok();
    Ok(())
}

/// Throws away input the picker did not consume.
///
/// Windows queues a release record for every press, and a key held down repeats.
/// Anything still queued when raw mode ends is handed to the shell, which reads
/// it as if the user had typed it at the prompt.
fn drain_input() {
    while event::poll(Duration::ZERO).unwrap_or(false) {
        if event::read().is_err() {
            return;
        }
    }
}

/// Leaves the terminal as it was found. Safe to call more than once.
fn restore() {
    let _ = disable_raw_mode();
    if let Ok(mut device) = terminal_device() {
        let _ = execute!(device, LeaveAlternateScreen);
    }
}

/// A panic inside raw mode would otherwise leave the user with an unusable
/// terminal and no visible message.
fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}
