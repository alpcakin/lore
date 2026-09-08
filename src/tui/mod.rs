//! The picker: a panel drawn below the prompt, driven by key events.

mod app;
mod form;
mod view;

pub use app::{App, Outcome};

use std::fs::File;
use std::panic;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::backend::{Backend, ClearType, CrosstermBackend};
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::console;

type Screen = Terminal<CrosstermBackend<File>>;

/// Rows the picker reserves below the prompt.
///
/// It opens as a panel under the command line rather than taking over the
/// screen, so the prompt and the scrollback above it stay where they were.
/// Ratatui clamps this to the terminal height.
const HEIGHT: u16 = 16;

/// Runs the picker and returns what the shell should do.
pub fn run(mut app: App) -> Result<Outcome> {
    let mut screen = enter()?;
    let mut top = top_of(&mut screen);
    let outcome = event_loop(&mut screen, &mut app, &mut top);
    leave(&mut screen, top)?;
    outcome
}

fn event_loop(screen: &mut Screen, app: &mut App, top: &mut u16) -> Result<Outcome> {
    loop {
        screen.draw(|frame| view::draw(app, frame))?;
        *top = (*top).min(top_of(screen));

        match event::read()? {
            // The next draw re-anchors the panel to wherever the cursor now is
            // and clears only where it lands, so the rows it is sitting on have
            // to be erased while the terminal still knows where they are.
            Event::Resize(..) => screen.clear()?,
            // Windows reports key releases as well as presses; acting on both
            // would process every keystroke twice.
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if let Some(outcome) = app.on_key(key)? {
                    return Ok(outcome);
                }
            }
            _ => {}
        }
    }
}

/// The screen row the panel currently starts on.
fn top_of(screen: &mut Screen) -> u16 {
    screen.get_frame().area().y
}

fn enter() -> Result<Screen> {
    install_panic_hook();
    enable_raw_mode().context("failed to put the terminal into raw mode")?;

    let mut screen = Terminal::with_options(
        CrosstermBackend::new(console::device()?),
        TerminalOptions {
            viewport: Viewport::Inline(HEIGHT),
        },
    )
    .context("failed to start the terminal backend")?;

    // The reserved rows still hold whatever the shell last printed there, and a
    // blank cell in the first frame matches a blank cell in the empty back
    // buffer, so the diff would never write over it.
    screen.clear().context("failed to clear the panel")?;
    screen.hide_cursor().ok();

    Ok(screen)
}

/// Erases the panel and leaves the cursor where it began, so the shell carries
/// on as though the picker had never drawn anything.
///
/// `top` is the highest row the panel ever occupied rather than the one it ends
/// on. A resize moves it, and the terminal's own clear only reaches the rows it
/// holds now, so anything left behind by the move has to be erased from here.
fn leave(screen: &mut Screen, top: u16) -> Result<()> {
    drain_input();

    let origin = screen.get_frame().area();
    let top = top.min(origin.y);

    screen.set_cursor_position((origin.x, top)).ok();
    screen
        .backend_mut()
        .clear_region(ClearType::AfterCursor)
        .ok();
    screen.show_cursor().ok();

    restore();
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
