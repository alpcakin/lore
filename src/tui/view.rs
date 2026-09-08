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

/// Colour carried across the selected row.
///
/// The bright shade rather than plain yellow: a legacy Windows console renders
/// the dark one close enough to its default foreground to read as no colour at
/// all.
const SELECTION: Color = Color::LightYellow;

/// Width of the selection marker, the pin and destructive slots, and the space
/// separating them from the command.
const MARKERS: usize = 5;

/// Most of a row the command column may take, however wide the commands are.
const COMMAND_CAP: usize = 60;

/// Share of the commands on screen the column is sized to hold in full.
const COMMAND_PERCENTILE: usize = 80;

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

    let selected = app.selected();
    let visible: Vec<RowText> = app
        .rows()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, row)| RowText {
            selected: index == selected,
            cmd: row.cmd.to_string(),
            desc: row.entry.desc.clone(),
            danger: row.entry.danger,
            pinned: row.pinned,
        })
        .collect();

    let columns = Columns::fit(&visible, area.width as usize);
    let lines: Vec<Line> = visible
        .into_iter()
        .map(|row| {
            let matched = app.highlight(&row.cmd);
            row_line(&row, &matched, columns)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// One row's content, gathered before drawing so the list can size its columns
/// against everything that is actually on screen.
struct RowText {
    selected: bool,
    cmd: String,
    desc: String,
    danger: bool,
    pinned: bool,
}

/// Column widths shared by every row of a frame.
#[derive(Clone, Copy)]
struct Columns {
    command: usize,
    room: usize,
}

impl Columns {
    /// Sizes the command column so that most commands on screen fit.
    ///
    /// Sizing to the longest would let one outlier hold the column open and push
    /// every description away from its command; the outlier is truncated
    /// instead.
    fn fit(rows: &[RowText], width: usize) -> Self {
        let room = width.saturating_sub(MARKERS);
        let cap = (room * COMMAND_CAP / 100).max(1);

        let mut lengths: Vec<usize> = rows.iter().map(|row| row.cmd.chars().count()).collect();
        lengths.sort_unstable();
        let typical = lengths
            .get(lengths.len() * COMMAND_PERCENTILE / 100)
            .or(lengths.last())
            .copied()
            .unwrap_or(0);

        Self {
            command: typical.clamp(1, cap),
            room,
        }
    }

    fn description(&self) -> usize {
        self.room.saturating_sub(self.command + GAP.len())
    }
}

/// One row: a marker column, then the command and description columns.
///
/// The markers occupy a fixed width whether or not the row has any, so a pinned
/// or destructive entry does not shunt its own columns out of line with the rest
/// of the list.
fn row_line(row: &RowText, matched: &[u32], columns: Columns) -> Line<'static> {
    // The selected row is carried in one colour from end to end. A wide gap
    // between a short command and its description otherwise makes the eye travel
    // the row to work out which belongs to which.
    let (command_style, description_style, accent) = if row.selected {
        let selected = Style::new().fg(SELECTION);
        (
            selected,
            selected,
            selected.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
    } else {
        (
            Style::new(),
            dim(),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )
    };

    // Pinning and danger get a slot each. Sharing one would let a preference
    // hide a warning, and the warning is the one thing worth reading before
    // pressing enter. Both keep their own colour when the row is selected.
    let mut spans = vec![
        Span::styled(
            if row.selected { SELECTED } else { UNSELECTED },
            command_style,
        ),
        Span::styled(
            if row.pinned { "*" } else { " " },
            Style::new().fg(Color::Yellow),
        ),
        Span::styled(
            if row.danger { "!" } else { " " },
            Style::new().fg(Color::Red),
        ),
        Span::raw(" "),
    ];

    let command = truncate(&row.cmd, columns.command);
    let padding = columns.command - command.chars().count();
    spans.extend(highlighted(&command, matched, command_style, accent));

    if columns.description() > ELLIPSIS.len() && !row.desc.is_empty() {
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(
            format!("{GAP}{}", truncate(&row.desc, columns.description())),
            description_style,
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
fn highlighted(text: &str, matched: &[u32], base: Style, accent: Style) -> Vec<Span<'static>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(selected: bool) -> RowText {
        RowText {
            selected,
            cmd: "git log".to_string(),
            desc: "Show history".to_string(),
            danger: false,
            pinned: false,
        }
    }

    fn columns() -> Columns {
        Columns {
            command: 20,
            room: 60,
        }
    }

    /// Every visible piece of the selected row carries the same colour, so the
    /// eye does not have to travel the gap to pair a command with its
    /// description.
    #[test]
    fn the_selected_row_is_one_colour_throughout() {
        let line = row_line(&row(true), &[], columns());

        for span in &line.spans {
            if span.content.trim().is_empty() {
                continue;
            }
            assert_eq!(
                span.style.fg,
                Some(SELECTION),
                "{:?} is not part of the selected colour",
                span.content
            );
        }
    }

    /// A pin is a preference and danger is a warning, so one must never hide
    /// the other.
    #[test]
    fn a_pinned_destructive_row_shows_both_markers() {
        let mut entry = row(false);
        entry.pinned = true;
        entry.danger = true;

        let drawn: String = row_line(&entry, &[], columns())
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert!(drawn.starts_with("  *! git log"), "drawn as {drawn:?}");
    }

    #[test]
    fn markers_hold_their_width_when_a_row_has_none() {
        let plain: String = row_line(&row(false), &[], columns())
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert!(plain.starts_with("     git log"), "drawn as {plain:?}");
    }

    #[test]
    fn an_unselected_row_leaves_its_command_alone() {
        let line = row_line(&row(false), &[], columns());
        let command = line
            .spans
            .iter()
            .find(|span| span.content.contains("git log"))
            .expect("the command is drawn");

        assert_eq!(command.style.fg, None);
    }
}
