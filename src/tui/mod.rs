//! The picker: an alternate screen on stderr, driven by key events.

mod app;
mod form;
mod view;

pub use app::{App, Outcome};

use std::io::{self, Stderr};
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

type Screen = Terminal<CrosstermBackend<Stderr>>;

/// Runs the picker and returns what the shell should do.
///
/// The interface is drawn on stderr because stdout carries the chosen command
/// back to the calling shell. Mixing the two would put escape sequences into the
/// prompt.
pub fn run(mut app: App) -> Result<Outcome> {
    let mut screen = enter()?;
    let outcome = event_loop(&mut screen, &mut app);
    leave(&mut screen)?;
    outcome
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

    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen).context("failed to open the alternate screen")?;

    Terminal::new(CrosstermBackend::new(stderr)).context("failed to start the terminal backend")
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
    let _ = execute!(io::stderr(), LeaveAlternateScreen);
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
