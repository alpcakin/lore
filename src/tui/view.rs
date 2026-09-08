//! Drawing the picker.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::params;
use crate::tui::app::{App, Mode};
use crate::tui::form::Form;

const SELECTED: &str = "> ";
const UNSELECTED: &str = "  ";

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
    let count = format!(" {} ", app.matches());
    let [input, total] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(count.len() as u16)])
            .areas(area);

    let prompt = Line::from(vec![
        Span::styled(SELECTED, Style::new().fg(Color::Cyan)),
        Span::styled(app.query(), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled("_", dim()),
    ]);
    frame.render_widget(Paragraph::new(prompt), input);
    frame.render_widget(Paragraph::new(Span::styled(count, dim())), total);
}

fn draw_browse(app: &mut App, frame: &mut Frame, area: Rect) {
    let [list, detail] = Layout::vertical([Constraint::Min(1), Constraint::Length(6)]).areas(area);

    draw_list(app, frame, list);
    draw_detail(app, frame, detail);
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
    let lines: Vec<Line> = visible
        .into_iter()
        .map(|(index, cmd, desc, danger, pinned)| {
            let matched = app.highlight(&cmd);
            row_line(index == selected, &cmd, &desc, danger, pinned, &matched)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line(
    selected: bool,
    cmd: &str,
    desc: &str,
    danger: bool,
    pinned: bool,
    matched: &[u32],
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

    spans.extend(highlighted(cmd, matched, base));
    spans.push(Span::styled(format!("   {desc}"), dim()));
    Line::from(spans)
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
    let block = Block::new().borders(Borders::TOP).border_style(dim());

    let Some(row) = app.selected_row() else {
        let empty = Paragraph::new(Span::styled("Nothing matches that query", dim())).block(block);
        frame.render_widget(empty, area);
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

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
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
