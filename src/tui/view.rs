//! Drawing the picker.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::model::Layer;
use crate::params;
use crate::search;
use crate::tui::app::{App, Mode};
use crate::tui::form::{Choice, Field, Form, Picker};

// The interface stays ASCII only. A legacy Windows console runs on the
// system code page, where box drawing characters arrive as mojibake.
const RULE: &str = "-";
const PROMPT: &str = "find: ";
const FILTER: &str = "filter: ";
const SELECTED: &str = "> ";
const UNSELECTED: &str = "  ";
const GAP: &str = "   ";
const ELLIPSIS: &str = "..";

/// Space between one hint and the next, wide enough that it cannot be read as
/// the single space inside one.
const HINT_GAP: &str = "   ";

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

/// Indent of the form block, and the width its labels are given.
const INDENT: &str = "  ";
const LABEL: usize = 14;

/// Widest a form input is drawn, however wide the terminal is.
///
/// A rail running the whole width of a large terminal reads as a rule rather
/// than as somewhere to type.
const RAIL_MAX: usize = 60;

/// Drawn under a form input so an empty field is still somewhere on the screen.
const RAIL: &str = "_";

/// Share of the matching commands the column is sized to hold in full.
const COMMAND_PERCENTILE: usize = 80;

pub fn draw(app: &App, frame: &mut Frame) {
    // The hints sit above the heading so that opening the picker reads top down:
    // what you can do, then where you are, then the results. The blank row keeps
    // them from being mistaken for part of the line below.
    let [hints, _gap, heading, body] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(frame.area());

    draw_hints(app, frame, hints);

    // Each screen puts its own one line on the heading row. The query used to be
    // drawn there in every mode, which left a live looking prompt sitting over a
    // form that did not accept a word of it.
    match app.mode() {
        Mode::Browse => {
            draw_query(app, frame, heading);
            draw_browse(app, frame, body);
        }
        Mode::Params { form, .. } | Mode::Save { form } => match &form.picking {
            Some(picker) => {
                draw_title(&picker.title, frame, heading);
                draw_picker(form, picker, frame, body);
            }
            None => {
                draw_title(&form.title, frame, heading);
                draw_form(form, frame, body);
            }
        },
    }
}

fn draw_title(title: &str, frame: &mut Frame, area: Rect) {
    let title = Span::styled(title.to_string(), Style::new().add_modifier(Modifier::BOLD));
    frame.render_widget(Paragraph::new(title), area);
}

fn draw_rule(frame: &mut Frame, area: Rect) {
    let rule = Span::styled(RULE.repeat(area.width as usize), dim());
    frame.render_widget(Paragraph::new(rule), area);
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
    draw_rule(frame, rule);

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
    let [rule, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    draw_rule(frame, rule);

    let rail = rail_width(body.width);
    let indent = " ".repeat(INDENT.len() + LABEL);
    let mut lines = vec![Line::from("")];
    let mut focused_at = 0;

    for (index, field) in form.fields.iter().enumerate() {
        if index == form.focused {
            focused_at = lines.len();
        }
        lines.push(field_line(field, index == form.focused, rail));

        if let Some(hint) = &field.hint {
            lines.push(Line::from(Span::styled(format!("{indent}{hint}"), dim())));
        }
        lines.push(Line::from(""));
    }

    // A form with enough placeholders to outgrow the panel would otherwise clip
    // the field being typed into.
    let scroll = focused_at.saturating_sub(body.height.saturating_sub(1) as usize) as u16;
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), body);
}

/// One field: its label, its value, and a rail running under the rest of the
/// room it has.
///
/// The rail is what makes an empty field a place rather than nothing at all, and
/// its right edge is where the value stops growing.
fn field_line(field: &Field, focused: bool, rail: usize) -> Line<'static> {
    let label = if focused {
        Style::new().fg(Color::Cyan)
    } else {
        dim()
    };

    let value = tail(&field.value, rail);
    let filled = value.chars().count();

    let mut spans = vec![
        Span::raw(INDENT),
        Span::styled(format!("{:<LABEL$}", field.label), label),
        Span::styled(
            value,
            if focused {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            },
        ),
    ];

    // The caret is the one undimmed link in the rail, so the eye lands on where
    // the next character goes rather than on the run of underscores.
    if focused && filled < rail {
        spans.push(Span::raw(RAIL));
    }
    let remaining = rail.saturating_sub(filled + usize::from(focused));
    spans.push(Span::styled(RAIL.repeat(remaining), dim()));

    Line::from(spans)
}

fn rail_width(width: u16) -> usize {
    let room = (width as usize).saturating_sub(INDENT.len() * 2 + LABEL);
    room.clamp(1, RAIL_MAX)
}

/// The last `width` characters, so a value longer than the rail shows the end
/// being typed rather than a beginning that no longer moves.
fn tail(text: &str, width: usize) -> String {
    let length = text.chars().count();
    if length <= width {
        return text.to_string();
    }
    match width.checked_sub(ELLIPSIS.len()) {
        Some(room) => ELLIPSIS.to_string() + &text.chars().skip(length - room).collect::<String>(),
        None => String::new(),
    }
}

/// The values the focused field is offering, with the filter above them.
fn draw_picker(form: &Form, picker: &Picker, frame: &mut Frame, area: Rect) {
    let [filter, rule, list] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(area);

    let typed = Line::from(vec![
        Span::styled(FILTER, Style::new().fg(Color::Cyan)),
        Span::styled(
            picker.filter.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(RAIL, dim()),
    ]);
    frame.render_widget(Paragraph::new(typed), filter);
    draw_rule(frame, rule);

    let choices = form.visible();
    if choices.is_empty() {
        let empty = Paragraph::new(Span::styled("Nothing matches that filter", dim()));
        frame.render_widget(empty, list);
        return;
    }

    let height = list.height as usize;
    let first = picker.selected.saturating_sub(height.saturating_sub(1));
    let room = (list.width as usize).saturating_sub(SELECTED.len());

    let lines: Vec<Line> = choices
        .into_iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, choice)| choice_line(choice, index == picker.selected, &picker.filter, room))
        .collect();

    frame.render_widget(Paragraph::new(lines), list);
}

/// One offered value, with whatever the caller had to say about it held to the
/// right of the row.
fn choice_line(choice: &Choice, selected: bool, filter: &str, room: usize) -> Line<'static> {
    let base = if selected {
        Style::new().fg(SELECTION)
    } else {
        Style::new()
    };
    let accent = if selected {
        base.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    };

    let note = choice.note.clone().unwrap_or_default();
    let width = room.saturating_sub(if note.is_empty() {
        0
    } else {
        note.chars().count() + GAP.len()
    });

    let value = truncate(&choice.value, width);
    let padding = width - value.chars().count();
    let matched = search::highlight(&value, filter);

    let mut spans = vec![Span::styled(
        if selected { SELECTED } else { UNSELECTED },
        base,
    )];
    spans.extend(highlighted(&value, &matched, base, accent));

    if !note.is_empty() {
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(
            format!("{GAP}{note}"),
            if selected { base } else { dim() },
        ));
    }

    Line::from(spans)
}

/// The chords, and whatever the last action had to say.
///
/// A status message takes the line while it lasts. Giving it a row of its own
/// would mean the list changing height the moment anything happened, and the
/// panel is short enough that the ground would visibly move under what is being
/// read.
fn draw_hints(app: &App, frame: &mut Frame, area: Rect) {
    // Centred, because left aligning it would put a second column of text hard
    // against the left edge above the query and read as another prompt.
    if let Some(status) = app.status() {
        let style = Style::new().fg(Color::Yellow);
        frame.render_widget(
            Paragraph::new(Span::styled(status.to_string(), style)).centered(),
            area,
        );
        return;
    }

    // The chord carries the colour the rest of the interface gives to what the
    // user types, so the line reads as pairs rather than one grey run.
    let mut spans = Vec::new();
    for (chord, action) in hints(app) {
        if !spans.is_empty() {
            spans.push(Span::raw(HINT_GAP));
        }
        spans.push(Span::styled(chord, Style::new().fg(Color::Cyan)));
        spans.push(Span::styled(format!(" {action}"), dim()));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)).centered(), area);
}

fn hints(app: &App) -> Vec<(&'static str, &'static str)> {
    match app.mode() {
        Mode::Browse => browse_hints(),
        Mode::Params { form, .. } | Mode::Save { form } if form.picking.is_some() => picker_hints(),
        Mode::Params { .. } => form_hints("back"),
        Mode::Save { .. } => form_hints("cancel"),
    }
}

/// Fixed, whatever the shell handed over. A line that changes shape depending on
/// what happened before the picker opened cannot be learned.
fn browse_hints() -> Vec<(&'static str, &'static str)> {
    vec![
        ("enter", "insert"),
        ("esc", "close"),
        ("^s", "save"),
        ("^p", "pin"),
        ("^x", "remove"),
    ]
}

fn form_hints(escape: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![("enter", "next"), ("esc", escape), ("^u", "clear")]
}

fn picker_hints() -> Vec<(&'static str, &'static str)> {
    vec![
        ("enter", "use"),
        ("esc", "back"),
        ("type", "filter"),
        ("^u", "clear"),
    ]
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

    /// The line clips rather than wraps, so the longest one any screen draws has
    /// to survive the narrowest terminal worth supporting.
    #[test]
    fn no_hint_line_outgrows_a_narrow_terminal() {
        let lines = [browse_hints(), form_hints("cancel"), picker_hints()];

        for hints in lines {
            let width = hints
                .iter()
                .map(|(chord, action)| chord.len() + 1 + action.len())
                .sum::<usize>()
                + HINT_GAP.len() * (hints.len() - 1);

            assert!(width <= 80, "{hints:?} is {width} columns");
        }
    }

    fn field(label: &str, value: &str) -> Field {
        Field::new(label, value)
    }

    fn drawn(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// A field with nothing in it still has to be somewhere on the screen.
    #[test]
    fn an_empty_field_is_drawn_as_a_rail() {
        let line = field_line(&field("description", ""), false, 10);
        assert_eq!(drawn(&line), format!("  {:<14}__________", "description"));
    }

    /// The caret is the one part of the rail the eye is meant to land on, so it
    /// stays out of the dim run the rest of the rail is drawn in.
    #[test]
    fn the_focused_field_holds_a_caret_at_the_end_of_its_value() {
        let line = field_line(&field("command", "ls"), true, 10);

        assert_eq!(drawn(&line), format!("  {:<14}ls________", "command"));
        let caret = line
            .spans
            .iter()
            .find(|span| span.content.as_ref() == RAIL)
            .expect("the caret is drawn");
        assert!(!caret.style.add_modifier.contains(Modifier::DIM));
    }

    /// The rail is a fixed width, so a long value has to give up its beginning
    /// rather than the end being typed.
    #[test]
    fn a_value_longer_than_the_rail_shows_its_end() {
        assert_eq!(tail("kubectl logs -f api-0", 10), "..-f api-0");
        assert_eq!(tail("ls", 10), "ls");
    }

    #[test]
    fn the_rail_never_outgrows_its_maximum() {
        assert_eq!(rail_width(500), RAIL_MAX);
        assert!(rail_width(30) < RAIL_MAX);
        assert!(rail_width(0) >= 1);
    }

    fn choice(note: Option<&str>) -> Choice {
        Choice::new("git log --oneline", note.map(str::to_string))
    }

    #[test]
    fn a_choice_carries_its_note_at_the_end_of_the_row() {
        let line = choice_line(&choice(Some("already saved")), false, "", 40);
        let drawn = drawn(&line);

        assert!(
            drawn.starts_with("  git log --oneline"),
            "drawn as {drawn:?}"
        );
        assert!(drawn.ends_with("already saved"), "drawn as {drawn:?}");
    }

    /// The same rule the list follows: one colour from end to end, so the eye
    /// does not travel the gap to pair a row with its note.
    #[test]
    fn the_selected_choice_is_one_colour_throughout() {
        let line = choice_line(&choice(Some("already saved")), true, "", 40);

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
}
