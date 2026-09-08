//! Drawing the picker.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::model::Layer;
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

/// Colour of a row that came with the binary rather than from the user's own
/// library.
///
/// Grey rather than dim, because the description column is dim already and
/// stacking the two leaves a legacy Windows console with nothing legible.
const BUILTIN: Color = Color::DarkGray;

/// Width of the selection marker, the pin and destructive slots, and the space
/// separating them from the command.
const MARKERS: usize = 5;

/// Rows the detail pane always occupies: a rule and four lines of content.
///
/// Enough for a command that wraps once, its description, and a placeholder or
/// two. Anything past that is clipped rather than allowed to move the list.
const DETAIL: u16 = 5;

/// Most of a row the command column may take, however wide the commands are.
const COMMAND_CAP: usize = 60;

/// Share of the matching commands the column is sized to hold in full.
const COMMAND_PERCENTILE: usize = 80;

pub fn draw(app: &App, frame: &mut Frame) {
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
    // Labelled rather than prefixed with the row marker, so it is obvious which
    // line accepts typing.
    let prompt = Line::from(vec![
        Span::styled(PROMPT, Style::new().fg(Color::Cyan)),
        Span::styled(app.query(), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled("_", dim()),
    ]);

    frame.render_widget(Paragraph::new(prompt), area);
}

fn draw_browse(app: &App, frame: &mut Frame, area: Rect) {
    // Fixed rather than sized to the selected entry. Letting it grow for an
    // entry with placeholders would change how many rows the list has every time
    // the cursor moved, and the ground would shift under what is being read.
    let detail = DETAIL.min(area.height.saturating_sub(1));
    let [list, detail] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(detail)]).areas(area);

    draw_list(app, frame, list);
    draw_detail(app, frame, detail);
}

fn draw_list(app: &App, frame: &mut Frame, area: Rect) {
    let height = area.height as usize;
    if height == 0 {
        return;
    }

    // Keep the cursor on screen without letting the window jump around.
    let first = app.selected().saturating_sub(height.saturating_sub(1));

    // Measured against every match rather than the rows on screen. Scrolling
    // changes which rows are visible, and sizing to those would slide the whole
    // table sideways every time the cursor moves past the edge.
    let lengths: Vec<usize> = app.rows().map(|row| row.cmd.chars().count()).collect();
    let columns = Columns::fit(&lengths, area.width as usize);

    let selected = app.selected();
    let lines: Vec<Line> = app
        .rows()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, row)| {
            let text = RowText {
                selected: index == selected,
                cmd: row.cmd.to_string(),
                desc: row.entry.desc.clone(),
                danger: row.entry.danger,
                pinned: row.pinned,
                layer: row.entry.layer,
            };
            let matched = app.highlight(&text.cmd);
            row_line(&text, &matched, columns)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// One row's content, owned so the line outlives the entry it came from.
struct RowText {
    selected: bool,
    cmd: String,
    desc: String,
    danger: bool,
    pinned: bool,
    layer: Layer,
}

/// Column widths shared by every row of a frame.
#[derive(Clone, Copy)]
struct Columns {
    command: usize,
    room: usize,
}

impl Columns {
    /// Sizes the command column so that most of the matching commands fit.
    ///
    /// Sizing to the longest would let one outlier hold the column open and push
    /// every description away from its command; the outlier is truncated
    /// instead.
    fn fit(lengths: &[usize], width: usize) -> Self {
        let room = width.saturating_sub(MARKERS);
        let cap = (room * COMMAND_CAP / 100).max(1);

        let mut lengths = lengths.to_vec();
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
    let accent = if row.selected {
        Style::new()
            .fg(SELECTION)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    };

    // Each row is carried in one colour from end to end. A wide gap between a
    // short command and its description otherwise makes the eye travel the row
    // to work out which belongs to which.
    let (command_style, description_style) = match (row.selected, row.layer) {
        (true, _) => {
            let selected = Style::new().fg(SELECTION);
            (selected, selected)
        }
        // The shipped set is a starting point, so it recedes and leaves the
        // foreground to whatever the user curated.
        (false, Layer::Builtin) => {
            let builtin = Style::new().fg(BUILTIN);
            (builtin, builtin)
        }
        (false, Layer::Project | Layer::User) => (Style::new(), dim()),
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
            let mut hints = vec!["enter insert", "esc close", "^n new", "^p pin", "^x remove"];
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
            layer: Layer::User,
        }
    }

    fn builtin(selected: bool) -> RowText {
        RowText {
            layer: Layer::Builtin,
            ..row(selected)
        }
    }

    fn command_colour(text: &RowText) -> Option<Color> {
        row_line(text, &[], columns())
            .spans
            .iter()
            .find(|span| span.content.contains("git log"))
            .expect("the command is drawn")
            .style
            .fg
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
        for text in [row(true), builtin(true)] {
            for span in &row_line(&text, &[], columns()).spans {
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

    /// Scrolling must not slide the table sideways, so the column is measured
    /// against every match rather than the window on screen.
    #[test]
    fn the_column_ignores_which_rows_are_on_screen() {
        let all = [10, 12, 14, 60, 11];
        let window = [10, 12];

        assert_ne!(
            Columns::fit(&all, 100).command,
            Columns::fit(&window, 100).command
        );
    }

    #[test]
    fn one_long_command_does_not_hold_the_column_open() {
        let lengths = [8, 9, 10, 11, 120];
        assert!(Columns::fit(&lengths, 100).command < 100);
    }

    #[test]
    fn a_command_of_your_own_is_drawn_plainly() {
        assert_eq!(command_colour(&row(false)), None);
    }

    /// The shipped set is a starting point rather than the point, so it has to
    /// be tellable from the user's own library at a glance.
    #[test]
    fn a_builtin_recedes_behind_what_the_user_saved() {
        assert_eq!(command_colour(&builtin(false)), Some(BUILTIN));
    }
}
