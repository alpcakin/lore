//! Drawing the picker.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::params;
use crate::tui::app::{App, Mode};
use crate::tui::form::Form;

// The interface stays ASCII only. A legacy Windows console runs on the
// system code page, where box drawing characters arrive as mojibake.
const RULE: &str = "-";
const PROMPT: &str = "find: ";
const SELECTED: &str = "> ";
const UNSELECTED: &str = "  ";
const GAP: &str = "   ";
const ELLIPSIS: &str = "..";

/// Percentage of a row given to the command, so descriptions line up in a
/// column rather than restarting wherever the command happens to end.
const COMMAND_SHARE: usize = 58;

pub fn draw(app: &mut App, frame: &mut Frame) {
    let [query, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_query(app, frame, query);

    match app.mode() {
        Mode::Browse => draw_browse(app, frame, body),
        Mode::Params { form, .. } | Mode::Save { form } => draw_form(form, frame, body),
    }

    draw_footer(app, frame, footer);
}

fn draw_query(app: &App, frame: &mut Frame, area: Rect) {
    let matches = match app.matches() {
        1 => "1 match ".to_string(),
        count => format!("{count} matches "),
    };
    let [input, total] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(matches.len() as u16)])
            .areas(area);

    // Labelled rather than prefixed with the row marker, so it is obvious which
    // line accepts typing.
    let prompt = Line::from(vec![
        Span::styled(PROMPT, Style::new().fg(Color::Cyan)),
        Span::styled(app.query(), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled("_", dim()),
    ]);
    frame.render_widget(Paragraph::new(prompt), input);
    frame.render_widget(Paragraph::new(Span::styled(matches, dim())), total);
}

fn draw_browse(app: &mut App, frame: &mut Frame, area: Rect) {
    let detail = detail_height(app).min(area.height.saturating_sub(1));
    let [list, detail] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(detail)]).areas(area);

    draw_list(app, frame, list);
    draw_detail(app, frame, detail);
}

/// The panel is short, so the detail pane only claims the rows it will fill.
fn detail_height(app: &App) -> u16 {
    let Some(row) = app.selected_row() else {
        return 2;
    };

    let rule = 1;
    let command_and_description = 2;
    let warning = u16::from(row.entry.danger);
    let placeholders = params::names(row.cmd).len() as u16;

    rule + command_and_description + warning + placeholders
}

fn draw_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let height = area.height as usize;
    if height == 0 {
        return;
    }

    // Keep the cursor on screen without letting the window jump around.
    let first = app.selected().saturating_sub(height.saturating_sub(1));

    let visible: Vec<(usize, String, String, bool, bool)> = app
        .rows()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, row)| {
            (
                index,
                row.cmd.to_string(),
                row.entry.desc.clone(),
                row.entry.danger,
                row.pinned,
            )
        })
        .collect();

    let selected = app.selected();
    let width = area.width as usize;
    let lines: Vec<Line> = visible
        .into_iter()
        .map(|(index, cmd, desc, danger, pinned)| {
            let matched = app.highlight(&cmd);
            row_line(
                index == selected,
                &cmd,
                &desc,
                danger,
                pinned,
                &matched,
                width,
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// One row: markers, the command, then as much of the description as fits.
///
/// Both halves are truncated deliberately rather than left to run off the right
/// edge, where the description would be the part lost every time.
fn row_line(
    selected: bool,
    cmd: &str,
    desc: &str,
    danger: bool,
    pinned: bool,
    matched: &[u32],
    width: usize,
) -> Line<'static> {
    let base = if selected {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };

    let mut spans = vec![Span::styled(
        if selected { SELECTED } else { UNSELECTED },
        Style::new().fg(Color::Cyan),
    )];

    if pinned {
        spans.push(Span::styled("* ", Style::new().fg(Color::Yellow)));
    }
    if danger {
        spans.push(Span::styled("! ", Style::new().fg(Color::Red)));
    }

    let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    let room = width.saturating_sub(used);

    // Both columns start at a fixed offset. Letting the description follow the
    // command directly would restart it at a different place on every row, which
    // is what makes a list of varying length commands unreadable.
    let command_width = (room * COMMAND_SHARE / 100).max(1);
    let command = truncate(cmd, command_width);
    let padding = command_width - command.chars().count();

    spans.extend(highlighted(&command, matched, base));

    let description_width = room.saturating_sub(command_width + GAP.len());
    if description_width > ELLIPSIS.len() && !desc.is_empty() {
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(
            format!("{GAP}{}", truncate(desc, description_width)),
            dim(),
        ));
    }

    Line::from(spans)
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    match width.checked_sub(ELLIPSIS.len()) {
        Some(room) => text.chars().take(room).collect::<String>() + ELLIPSIS,
        None => String::new(),
    }
}

/// Splits `text` into runs of matched and unmatched characters.
///
/// One span per character would work, but every span becomes its own cursor
/// move in the rendered output, so a long command turns into a great deal of
/// terminal traffic on each keystroke.
fn highlighted(text: &str, matched: &[u32], base: Style) -> Vec<Span<'static>> {
    let accent = base.fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_matched = false;

    for (index, character) in text.chars().enumerate() {
        let is_match = matched.binary_search(&(index as u32)).is_ok();
        if is_match != run_matched && !run.is_empty() {
            let style = if run_matched { accent } else { base };
            spans.push(Span::styled(std::mem::take(&mut run), style));
        }
        run_matched = is_match;
        run.push(character);
    }

    if !run.is_empty() {
        let style = if run_matched { accent } else { base };
        spans.push(Span::styled(run, style));
    }

    spans
}

fn draw_detail(app: &App, frame: &mut Frame, area: Rect) {
    let [rule, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new(Span::styled(RULE.repeat(rule.width as usize), dim())),
        rule,
    );

    let Some(row) = app.selected_row() else {
        let empty = Paragraph::new(Span::styled("Nothing matches that query", dim()));
        frame.render_widget(empty, body);
        return;
    };

    let mut lines = vec![
        Line::from(Span::styled(
            row.cmd.to_string(),
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(row.entry.desc.clone(), dim())),
    ];

    if row.entry.danger {
        lines.push(Line::from(Span::styled(
            "This command is destructive",
            Style::new().fg(Color::Red),
        )));
    }

    for name in params::names(row.cmd) {
        let desc = row
            .entry
            .params
            .get(&name)
            .and_then(|spec| spec.desc.as_deref())
            .unwrap_or("no description");
        lines.push(Line::from(vec![
            Span::styled(format!("<{name}> "), Style::new().fg(Color::Cyan)),
            Span::styled(desc.to_string(), dim()),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
}

fn draw_form(form: &Form, frame: &mut Frame, area: Rect) {
    let mut lines = vec![
        Line::from(Span::styled(
            form.title.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (index, field) in form.fields.iter().enumerate() {
        let focused = index == form.focused;
        let mut spans = vec![
            Span::styled(
                if focused { SELECTED } else { UNSELECTED },
                Style::new().fg(Color::Cyan),
            ),
            Span::styled(format!("{:<14}", field.label), dim()),
            Span::styled(
                field.value.clone(),
                if focused {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                },
            ),
        ];
        if focused {
            spans.push(Span::styled("_", dim()));
        }
        lines.push(Line::from(spans));

        if let Some(hint) = &field.hint {
            lines.push(Line::from(Span::styled(format!("      {hint}"), dim())));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn draw_footer(app: &App, frame: &mut Frame, area: Rect) {
    if let Some(status) = app.status() {
        let style = Style::new().fg(Color::Yellow);
        frame.render_widget(
            Paragraph::new(Span::styled(status.to_string(), style)),
            area,
        );
        return;
    }

    let hints = match app.mode() {
        Mode::Browse => {
            let mut hints = vec!["enter insert", "esc close", "^n new", "^p pin"];
            if app.has_last_command() {
                hints.insert(2, "^s save last");
            }
            hints.join("   ")
        }
        Mode::Params { .. } => "enter next   esc back   ^u clear".to_string(),
        Mode::Save { .. } => "enter next   esc cancel   ^u clear".to_string(),
    };

    frame.render_widget(Paragraph::new(Span::styled(hints, dim())), area);
}

fn dim() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}
