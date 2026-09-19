//! Picker state and key handling, kept free of any terminal so it can be tested
//! directly.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::{CommandBody, Entry, Layer, ParamSpec, ShellFamily};
use crate::params;
use crate::search::{self, Candidate};
use crate::store::definitions::{self, NewEntry};
use crate::store::stats::{Score, Stats};
use crate::tui::form::{Field, Form};

/// Shown when saving is asked to go on without a command.
const NO_COMMAND: &str = "Type a command, or press up for the ones you ran";

/// Commands from the shell's history that saving can walk back through.
///
/// Deep enough to reach past a run of throwaway commands, as a shell's own up
/// arrow would.
const HISTORY_LIMIT: usize = 50;

/// What the picker hands back to the shell.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Command to place in the prompt. Running it stays the user's decision.
    Insert {
        command: String,
        /// Where to leave the cursor, in characters from the start of the
        /// command. `None` puts it at the end, which is where a finished
        /// command wants it.
        cursor: Option<usize>,
    },
    Cancelled,
}

impl Outcome {
    #[cfg(test)]
    fn insert(command: &str) -> Self {
        Self::Insert {
            command: command.to_string(),
            cursor: None,
        }
    }
}

pub enum Mode {
    Browse,
    Params {
        entry_id: String,
        template: String,
        form: Form,
    },
    Save(Save),
    /// Rewriting an entry in place. Everything the form does not show is
    /// carried through untouched, so editing a description cannot lose a
    /// per shell variant or a placeholder's documentation.
    Edit {
        id: String,
        cmd: CommandBody,
        params: BTreeMap<String, ParamSpec>,
        danger: bool,
        /// A builtin is rewritten as a user entry that shadows it rather than
        /// changed where it lives.
        shadowing: bool,
        form: Form,
    },
}

/// Saving a command, asked the way a shell would ask it: the command on one
/// line, then what it is for on the next. There is no form to fill in, and
/// tags are either written into the answer as `#tag` or taken from the
/// command's own words.
pub struct Save {
    pub step: SaveStep,
    pub command: String,
    pub purpose: String,
    /// The history entry the command line is showing while up and down walk
    /// through it. `None` once the line is past the newest, as in a shell.
    pub recalled: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStep {
    Command,
    Purpose,
}

impl Save {
    fn line(&mut self) -> &mut String {
        match self.step {
            SaveStep::Command => &mut self.command,
            SaveStep::Purpose => &mut self.purpose,
        }
    }
}

pub struct App {
    entries: Vec<Entry>,
    family: ShellFamily,
    /// Entries that have a command for this shell, in load order.
    pickable: Vec<usize>,
    /// Ranked entry indices, best first.
    order: Vec<usize>,
    selected: usize,
    query: String,
    mode: Mode,
    status: Option<String>,
    stats: Stats,
    scores: HashMap<String, Score>,
    library: PathBuf,
    /// What the shell last ran, newest first, offered when saving.
    history: Vec<String>,
    /// Entry the next ctrl+x will actually remove.
    armed_to_remove: Option<String>,
    /// Set once the library file has been written, so the caller knows there
    /// is something to sync.
    changed: bool,
    now: i64,
}

impl App {
    pub fn new(
        entries: Vec<Entry>,
        family: ShellFamily,
        stats: Stats,
        library: PathBuf,
        history: Vec<String>,
        now: i64,
    ) -> Result<Self> {
        let scores = stats.scores(now)?;
        let mut app = Self {
            entries,
            family,
            pickable: Vec::new(),
            order: Vec::new(),
            selected: 0,
            query: String::new(),
            mode: Mode::Browse,
            status: None,
            stats,
            scores,
            library,
            history: prepare_history(history),
            armed_to_remove: None,
            changed: false,
            now,
        };
        app.reindex();
        Ok(app)
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn matches(&self) -> usize {
        self.order.len()
    }

    /// The entries currently on offer, best first.
    pub fn rows(&self) -> impl Iterator<Item = Row<'_>> {
        self.order.iter().map(move |&index| {
            let entry = &self.entries[index];
            Row {
                entry,
                cmd: entry.cmd_for(self.family).expect("filtered on this"),
                pinned: self.scores.get(&entry.id).is_some_and(|s| s.pinned),
            }
        })
    }

    pub fn selected_row(&self) -> Option<Row<'_>> {
        self.rows().nth(self.selected)
    }

    /// Highlights the query inside a haystack for the rows on screen.
    pub fn highlight(&self, haystack: &str) -> Vec<u32> {
        search::highlight(haystack, &self.query)
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Result<Option<Outcome>> {
        self.status = None;
        // Any other keystroke stands the confirmation down.
        let armed = self.armed_to_remove.take();

        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if control && matches!(key.code, KeyCode::Char('c')) {
            return Ok(Some(Outcome::Cancelled));
        }

        match &mut self.mode {
            Mode::Browse => self.browse_key(key, control, armed),
            Mode::Params { .. } => self.params_key(key, control),
            Mode::Save { .. } => self.save_key(key, control),
            Mode::Edit { .. } => self.edit_key(key, control),
        }
    }

    fn browse_key(
        &mut self,
        key: KeyEvent,
        control: bool,
        armed: Option<String>,
    ) -> Result<Option<Outcome>> {
        match key.code {
            KeyCode::Esc => return Ok(Some(Outcome::Cancelled)),
            KeyCode::Char('g') if control => return Ok(Some(Outcome::Cancelled)),
            KeyCode::Enter => return self.choose(),

            KeyCode::Up => self.move_by(-1),
            KeyCode::Down => self.move_by(1),
            KeyCode::Char('k') if control => self.move_by(-1),
            KeyCode::Char('j') if control => self.move_by(1),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::PageDown => self.move_by(10),

            KeyCode::Char('p') if control => self.toggle_pin()?,
            KeyCode::Char('s') if control => self.begin_save(),
            KeyCode::Char('e') if control => self.begin_edit(),
            KeyCode::Char('x') if control => self.remove(armed)?,

            KeyCode::Char('u') if control => {
                self.query.clear();
                self.reindex();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.reindex();
            }
            KeyCode::Char(character) if !control => {
                self.query.push(character);
                self.reindex();
            }
            _ => {}
        }

        Ok(None)
    }

    fn params_key(&mut self, key: KeyEvent, control: bool) -> Result<Option<Outcome>> {
        let Mode::Params { form, .. } = &mut self.mode else {
            return Ok(None);
        };

        match key.code {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Enter | KeyCode::Tab | KeyCode::Down => {
                if form.advance() {
                    return self.finish_params();
                }
            }
            KeyCode::Up | KeyCode::BackTab => form.retreat(),
            KeyCode::Char('u') if control => form.clear(),
            KeyCode::Backspace => form.backspace(),
            KeyCode::Char(character) if !control => form.insert(character),
            _ => {}
        }

        Ok(None)
    }

    fn save_key(&mut self, key: KeyEvent, control: bool) -> Result<Option<Outcome>> {
        let Mode::Save(save) = &mut self.mode else {
            return Ok(None);
        };

        match (save.step, key.code) {
            (_, KeyCode::Esc) => self.mode = Mode::Browse,
            // Submits from wherever the cursor is. The checks in finish_save
            // still apply, and send the cursor to whatever is missing.
            (_, KeyCode::Char('s')) if control => self.finish_save()?,

            (SaveStep::Command, KeyCode::Up) => self.recall(1),
            (SaveStep::Command, KeyCode::Down) => self.recall(-1),
            (SaveStep::Command, KeyCode::Enter | KeyCode::Tab) => {
                if save.command.trim().is_empty() {
                    self.status = Some(NO_COMMAND.to_string());
                } else {
                    save.step = SaveStep::Purpose;
                }
            }

            (SaveStep::Purpose, KeyCode::Enter) => self.finish_save()?,
            (SaveStep::Purpose, KeyCode::Up | KeyCode::BackTab) => {
                save.step = SaveStep::Command;
            }

            (_, KeyCode::Char('u')) if control => save.line().clear(),
            (_, KeyCode::Backspace) => {
                save.line().pop();
            }
            (_, KeyCode::Char(character)) if !control => save.line().push(character),
            _ => {}
        }

        Ok(None)
    }

    /// Walks the command line through the shell's history, `older` steps back.
    ///
    /// Past the newest entry the line is empty, the way a shell's own prompt is
    /// when down is pressed at the bottom of its history.
    fn recall(&mut self, older: isize) {
        let Mode::Save(save) = &mut self.mode else {
            return;
        };

        let next = match save.recalled {
            Some(index) => index as isize + older,
            None if older > 0 => 0,
            None => return,
        };

        if next < 0 {
            save.recalled = None;
            save.command.clear();
        } else if let Some(command) = self.history.get(next as usize) {
            save.recalled = Some(next as usize);
            save.command = command.clone();
        }
    }

    fn edit_key(&mut self, key: KeyEvent, control: bool) -> Result<Option<Outcome>> {
        let Mode::Edit { form, .. } = &mut self.mode else {
            return Ok(None);
        };

        match key.code {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Enter | KeyCode::Tab | KeyCode::Down => {
                if form.advance() {
                    self.finish_edit()?;
                }
            }
            KeyCode::Up | KeyCode::BackTab => form.retreat(),
            KeyCode::Char('s') if control => self.finish_edit()?,
            KeyCode::Char('u') if control => form.clear(),
            KeyCode::Backspace => form.backspace(),
            KeyCode::Char(character) if !control => form.insert(character),
            _ => {}
        }

        Ok(None)
    }

    /// Enter on a row: prompt for placeholders, or hand the command back.
    fn choose(&mut self) -> Result<Option<Outcome>> {
        let Some(row) = self.selected_row() else {
            return Ok(None);
        };
        let entry_id = row.entry.id.clone();
        let template = row.cmd.to_string();

        let names = params::names(&template);
        if names.is_empty() {
            self.stats.record_use(&entry_id, self.now)?;
            return Ok(Some(Outcome::Insert {
                command: template,
                cursor: None,
            }));
        }

        // One placeholder is not worth a screen. The command goes to the prompt
        // with the placeholder cut out and the cursor in the gap, so the value
        // is typed against the shell's own completion rather than into a form
        // that knows nothing about paths, branches or container names.
        if let [placeholder] = params::parse(&template).as_slice() {
            let span = placeholder.span.clone();
            let cursor = template[..span.start].chars().count();
            let mut command = template;
            command.replace_range(span, "");

            self.stats.record_use(&entry_id, self.now)?;
            return Ok(Some(Outcome::Insert {
                command,
                cursor: Some(cursor),
            }));
        }

        let remembered = self.stats.last_params(&entry_id)?;
        let defaults: BTreeMap<String, String> = params::parse(&template)
            .into_iter()
            .filter_map(|p| p.default.map(|value| (p.name, value)))
            .collect();

        let fields = names
            .iter()
            .map(|name| {
                let value = remembered
                    .get(name)
                    .or_else(|| defaults.get(name))
                    .cloned()
                    .unwrap_or_default();
                let hint = self
                    .entries
                    .iter()
                    .find(|e| e.id == entry_id)
                    .and_then(|e| e.params.get(name))
                    .and_then(|spec| spec.desc.clone());
                Field::new(name, value).with_hint(hint)
            })
            .collect();

        self.mode = Mode::Params {
            entry_id,
            template,
            form: Form::new("Fill in the placeholders", fields),
        };
        Ok(None)
    }

    fn finish_params(&mut self) -> Result<Option<Outcome>> {
        let Mode::Params {
            entry_id,
            template,
            form,
        } = &self.mode
        else {
            return Ok(None);
        };

        let mut values = BTreeMap::new();
        for field in &form.fields {
            values.insert(field.label.clone(), field.value.trim().to_string());
        }

        let command = params::render(template, &values);
        let entry_id = entry_id.clone();

        for (name, value) in &values {
            self.stats.remember_param(&entry_id, name, value)?;
        }
        self.stats.record_use(&entry_id, self.now)?;

        Ok(Some(Outcome::Insert {
            command,
            cursor: None,
        }))
    }

    /// Starts saving on the newest thing the shell has.
    ///
    /// The shell puts whatever was on the prompt line ahead of its history, so
    /// a command typed but not yet run is what gets offered first, and the
    /// last one run when the line was empty.
    fn begin_save(&mut self) {
        self.mode = Mode::Save(Save {
            step: SaveStep::Command,
            command: self.history.first().cloned().unwrap_or_default(),
            purpose: String::new(),
            recalled: (!self.history.is_empty()).then_some(0),
        });
    }

    fn finish_save(&mut self) -> Result<()> {
        let Mode::Save(save) = &mut self.mode else {
            return Ok(());
        };

        let command = save.command.trim().to_string();
        let (description, given) = definitions::split_purpose(&save.purpose);

        if command.is_empty() {
            save.step = SaveStep::Command;
            self.status = Some(NO_COMMAND.to_string());
            return Ok(());
        }
        // Without a description the entry is only findable by its own text,
        // which defeats the point of saving it in the first place.
        if description.is_empty() {
            save.step = SaveStep::Purpose;
            self.status = Some("Say what it is for, so you can find it later".to_string());
            return Ok(());
        }

        let taken: BTreeSet<String> = self.entries.iter().map(|e| e.id.clone()).collect();
        let entry = NewEntry {
            id: definitions::suggest_id(&command, &taken),
            tags: definitions::merge_tags(given, &command),
            cmd: CommandBody::Shared(command),
            desc: description,
            params: BTreeMap::new(),
            danger: false,
        };
        let id = entry.id.clone();

        definitions::append(&self.library, &entry)?;
        self.stats.record_new(&id, self.now)?;

        self.entries = definitions::load(Some(&self.library))?;
        self.scores = self.stats.scores(self.now)?;
        self.mode = Mode::Browse;
        self.query.clear();
        self.reindex();
        self.select_id(&id);
        self.status = Some(format!("Saved as {id}"));
        self.changed = true;

        Ok(())
    }

    /// Whether the library already holds exactly this command for this shell.
    pub fn is_saved(&self, command: &str) -> bool {
        let command = command.trim();
        self.entries
            .iter()
            .any(|entry| entry.cmd_for(self.family) == Some(command))
    }

    /// Tags the entry being saved would get, for showing before it is saved.
    pub fn save_tags(&self) -> Vec<String> {
        let Mode::Save(save) = &self.mode else {
            return Vec::new();
        };
        let (_, given) = definitions::split_purpose(&save.purpose);
        definitions::merge_tags(given, save.command.trim())
    }

    /// Whether anything in the library was written while the picker was open.
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// Opens the edit screen on the selected entry.
    ///
    /// The id is not offered: changing it would orphan everything the usage
    /// statistics have learned about the entry, and the entry can be removed
    /// and saved again if it really needs a different one.
    fn begin_edit(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let entry = row.entry;

        let form = Form::new(
            format!("Edit {}", entry.id),
            vec![
                Field::new("command", row.cmd.to_string()),
                Field::new("description", entry.desc.clone()),
                Field::new("tags", entry.tags.join(", "))
                    .with_hint(Some("comma separated".to_string())),
            ],
        );

        self.mode = Mode::Edit {
            id: entry.id.clone(),
            cmd: entry.cmd.clone(),
            params: entry.params.clone(),
            danger: entry.danger,
            shadowing: entry.layer != Layer::User,
            form,
        };
    }

    fn finish_edit(&mut self) -> Result<()> {
        let Mode::Edit {
            id,
            cmd,
            params,
            danger,
            shadowing,
            form,
        } = &self.mode
        else {
            return Ok(());
        };

        let command = form.value(0).to_string();
        let description = form.value(1).to_string();

        if command.is_empty() {
            self.status = Some("A command is required".to_string());
            return Ok(());
        }
        if description.is_empty() {
            self.status = Some("A description is required to find this later".to_string());
            if let Mode::Edit { form, .. } = &mut self.mode {
                form.focused = 1;
            }
            return Ok(());
        }

        // Only the variant for the shell being used is replaced. The others
        // were never on screen and are none of this edit's business.
        let cmd = match cmd {
            CommandBody::Shared(_) => CommandBody::Shared(command),
            CommandBody::PerShell(variants) => {
                let mut variants = variants.clone();
                variants.insert(self.family, command);
                CommandBody::PerShell(variants)
            }
        };

        let entry = NewEntry {
            id: id.clone(),
            cmd,
            desc: description,
            tags: definitions::parse_tags(form.value(2)),
            params: params.clone(),
            danger: *danger,
        };
        let id = entry.id.clone();
        let shadowing = *shadowing;

        definitions::upsert(&self.library, &entry)?;
        self.changed = true;

        self.entries = definitions::load(Some(&self.library))?;
        self.mode = Mode::Browse;
        self.reindex();
        self.select_id(&id);
        self.status = Some(if shadowing {
            format!("Saved {id} to your library, overriding the builtin")
        } else {
            format!("Updated {id}")
        });

        Ok(())
    }

    /// Takes an entry out of the picker, asking once before it does.
    ///
    /// A builtin lives inside the binary and cannot be deleted, so it is added
    /// to the user's disabled list instead. Either way it stops appearing, which
    /// is what was asked for.
    fn remove(&mut self, armed: Option<String>) -> Result<()> {
        let Some(row) = self.selected_row() else {
            return Ok(());
        };
        let id = row.entry.id.clone();
        let own = row.entry.layer == Layer::User;

        if armed.as_deref() != Some(id.as_str()) {
            self.status = Some(format!("Remove {id}? ctrl+x again to confirm"));
            self.armed_to_remove = Some(id);
            return Ok(());
        }

        if own {
            definitions::remove(&self.library, &id)?;
        } else {
            definitions::disable(&self.library, &id)?;
        }
        self.stats.forget(&id)?;

        self.changed = true;
        self.entries = definitions::load(Some(&self.library))?;
        self.scores = self.stats.scores(self.now)?;
        self.reindex();
        self.status = Some(if own {
            format!("Removed {id}")
        } else {
            format!("Hid {id}, listed under disabled in your library")
        });

        Ok(())
    }

    fn toggle_pin(&mut self) -> Result<()> {
        let Some(row) = self.selected_row() else {
            return Ok(());
        };
        let id = row.entry.id.clone();
        let pinned = !row.pinned;

        self.stats.set_pinned(&id, pinned, self.now)?;
        self.scores = self.stats.scores(self.now)?;
        self.reindex();
        self.select_id(&id);
        self.status = Some(if pinned { "Pinned" } else { "Unpinned" }.to_string());

        Ok(())
    }

    fn move_by(&mut self, delta: isize) {
        if self.order.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.order.len() - 1;
        let target = self.selected as isize + delta;
        self.selected = target.clamp(0, last as isize) as usize;
    }

    fn select_id(&mut self, id: &str) {
        if let Some(position) = self
            .order
            .iter()
            .position(|&index| self.entries[index].id == id)
        {
            self.selected = position;
        }
    }

    /// Recomputes which entries are on offer and in what order.
    fn reindex(&mut self) {
        self.pickable = (0..self.entries.len())
            .filter(|&index| self.entries[index].cmd_for(self.family).is_some())
            .collect();

        let candidates: Vec<Candidate<'_>> = self
            .pickable
            .iter()
            .map(|&index| {
                let entry = &self.entries[index];
                Candidate {
                    entry,
                    cmd: entry.cmd_for(self.family).expect("filtered on this"),
                }
            })
            .collect();

        let ranked = search::rank(&candidates, &self.scores, &self.query);
        self.order = ranked.into_iter().map(|rank| self.pickable[rank]).collect();
        self.selected = self.selected.min(self.order.len().saturating_sub(1));
    }
}

/// Trims what the shell handed over, drops blanks and repeats, and caps the
/// list.
///
/// A shell history is mostly the same handful of commands run again and again,
/// and a list where nine rows in ten are `cd ..` is not worth opening.
fn prepare_history(raw: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();

    raw.into_iter()
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty() && seen.insert(command.clone()))
        .take(HISTORY_LIMIT)
        .collect()
}

/// One line of the picker.
pub struct Row<'a> {
    pub entry: &'a Entry,
    pub cmd: &'a str,
    pub pinned: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommandBody, Layer, ParamSpec};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    fn typed(app: &mut App, text: &str) {
        for character in text.chars() {
            app.on_key(key(KeyCode::Char(character))).unwrap();
        }
    }

    fn entry(id: &str, cmd: &str, desc: &str) -> Entry {
        Entry {
            id: id.to_string(),
            cmd: CommandBody::Shared(cmd.to_string()),
            desc: desc.to_string(),
            tags: Vec::new(),
            params: BTreeMap::new(),
            danger: false,
            layer: Layer::Builtin,
        }
    }

    fn app_with(entries: Vec<Entry>) -> App {
        app_with_history(entries, vec!["kics scan -p .".to_string()])
    }

    fn app_with_history(entries: Vec<Entry>, history: Vec<String>) -> App {
        let library = std::env::temp_dir().join(format!(
            "lore-app-{}-{:?}.yaml",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&library);
        App::new(
            entries,
            ShellFamily::Posix,
            Stats::in_memory().unwrap(),
            library,
            history,
            0,
        )
        .unwrap()
    }

    fn history_sample() -> App {
        app_with_history(
            vec![entry("git.log", "git log --oneline", "Show history")],
            vec![
                "kics scan -p .".to_string(),
                "docker compose up -d".to_string(),
                "git log --oneline".to_string(),
            ],
        )
    }

    fn sample() -> App {
        app_with(vec![
            entry("git.log", "git log --oneline", "Show history"),
            entry("docker.ps", "docker ps -a", "List containers"),
            entry(
                "k8s.logs",
                "kubectl logs -f <pod> -n <namespace:default>",
                "Follow pod logs",
            ),
        ])
    }

    #[test]
    fn typing_filters_the_list() {
        let mut app = sample();
        assert_eq!(app.matches(), 3);
        typed(&mut app, "git");
        assert_eq!(app.matches(), 1);
        assert_eq!(app.selected_row().unwrap().entry.id, "git.log");
    }

    #[test]
    fn backspace_widens_the_list_again() {
        let mut app = sample();
        typed(&mut app, "git");
        app.on_key(key(KeyCode::Backspace)).unwrap();
        app.on_key(key(KeyCode::Backspace)).unwrap();
        app.on_key(key(KeyCode::Backspace)).unwrap();
        assert_eq!(app.query(), "");
        assert_eq!(app.matches(), 3);
    }

    #[test]
    fn selection_stays_inside_the_list() {
        let mut app = sample();
        for _ in 0..10 {
            app.on_key(key(KeyCode::Down)).unwrap();
        }
        assert_eq!(app.selected(), 2);
        for _ in 0..10 {
            app.on_key(key(KeyCode::Up)).unwrap();
        }
        assert_eq!(app.selected(), 0);
    }

    #[test]
    fn a_narrowed_list_never_leaves_the_cursor_past_the_end() {
        let mut app = sample();
        app.on_key(key(KeyCode::Down)).unwrap();
        app.on_key(key(KeyCode::Down)).unwrap();
        typed(&mut app, "git");
        assert_eq!(app.selected(), 0);
        assert!(app.selected_row().is_some());
    }

    #[test]
    fn enter_on_a_plain_command_returns_it() {
        let mut app = sample();
        typed(&mut app, "docker");
        let outcome = app.on_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(outcome, Some(Outcome::insert("docker ps -a")));
    }

    #[test]
    fn escape_cancels_without_choosing() {
        let mut app = sample();
        assert_eq!(
            app.on_key(key(KeyCode::Esc)).unwrap(),
            Some(Outcome::Cancelled)
        );
    }

    #[test]
    fn the_opening_chord_also_closes_the_picker() {
        let mut app = sample();
        assert_eq!(app.on_key(ctrl('g')).unwrap(), Some(Outcome::Cancelled));
    }

    #[test]
    fn enter_on_a_parameterised_command_asks_for_values() {
        let mut app = sample();
        typed(&mut app, "kubectl");
        assert_eq!(app.on_key(key(KeyCode::Enter)).unwrap(), None);
        assert!(matches!(app.mode(), Mode::Params { .. }));
    }

    #[test]
    fn placeholder_defaults_arrive_pre_filled() {
        let mut app = sample();
        typed(&mut app, "kubectl");
        app.on_key(key(KeyCode::Enter)).unwrap();

        let Mode::Params { form, .. } = app.mode() else {
            panic!("expected the parameter form");
        };
        assert_eq!(form.fields[0].label, "pod");
        assert_eq!(form.fields[0].value, "");
        assert_eq!(form.fields[1].label, "namespace");
        assert_eq!(form.fields[1].value, "default");
    }

    #[test]
    fn filled_placeholders_produce_the_final_command() {
        let mut app = sample();
        typed(&mut app, "kubectl");
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(&mut app, "api-0");
        app.on_key(key(KeyCode::Enter)).unwrap();
        let outcome = app.on_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(
            outcome,
            Some(Outcome::insert("kubectl logs -f api-0 -n default"))
        );
    }

    #[test]
    fn a_second_use_remembers_the_last_values() {
        let mut app = sample();
        typed(&mut app, "kubectl");
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(&mut app, "api-0");
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();

        // Reopening the same entry should not ask for the pod name again.
        app.mode = Mode::Browse;
        app.on_key(key(KeyCode::Enter)).unwrap();
        let Mode::Params { form, .. } = app.mode() else {
            panic!("expected the parameter form");
        };
        assert_eq!(form.fields[0].value, "api-0");
    }

    #[test]
    fn escape_leaves_the_parameter_form_without_choosing() {
        let mut app = sample();
        typed(&mut app, "kubectl");
        app.on_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.on_key(key(KeyCode::Esc)).unwrap(), None);
        assert!(matches!(app.mode(), Mode::Browse));
    }

    #[test]
    fn parameter_descriptions_reach_the_form() {
        let mut with_desc = entry(
            "two.params",
            "scan -p <path> --format <format>",
            "Scan a directory",
        );
        with_desc.params.insert(
            "path".to_string(),
            ParamSpec {
                desc: Some("Directory to scan".to_string()),
                from: None,
            },
        );

        let mut app = app_with(vec![with_desc]);
        app.on_key(key(KeyCode::Enter)).unwrap();
        let Mode::Params { form, .. } = app.mode() else {
            panic!("expected the parameter form");
        };
        assert_eq!(form.fields[0].hint.as_deref(), Some("Directory to scan"));
    }

    /// A form is a poor place to type a path or a branch name: it knows nothing
    /// the shell's own completion knows. One placeholder goes to the prompt
    /// instead, with the cursor sitting in the gap it left.
    #[test]
    fn a_single_placeholder_lands_in_the_prompt_under_the_cursor() {
        let mut app = app_with(vec![entry(
            "one.param",
            "scan -p <path>",
            "Scan a directory",
        )]);
        let outcome = app.on_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(
            outcome,
            Some(Outcome::Insert {
                command: "scan -p ".to_string(),
                cursor: Some(8),
            })
        );
    }

    /// The shortcut is about typing one value, not about skipping the form. A
    /// placeholder used twice still has to be filled in one place.
    #[test]
    fn a_placeholder_used_twice_still_opens_the_form() {
        let mut app = app_with(vec![entry(
            "twice",
            "mv <name> <name>.bak",
            "Back a file up in place",
        )]);
        assert_eq!(app.on_key(key(KeyCode::Enter)).unwrap(), None);
        assert!(matches!(app.mode(), Mode::Params { .. }));
    }

    #[test]
    fn a_command_with_no_placeholder_leaves_the_cursor_alone() {
        let mut app = sample();
        typed(&mut app, "docker");

        let Some(Outcome::Insert { cursor, .. }) = app.on_key(key(KeyCode::Enter)).unwrap() else {
            panic!("expected a command");
        };
        assert_eq!(cursor, None);
    }

    fn save_mode(app: &App) -> &Save {
        let Mode::Save(save) = app.mode() else {
            panic!("expected to be saving");
        };
        save
    }

    #[test]
    fn saving_starts_on_the_newest_command_and_asks_what_it_is_for() {
        let mut app = sample();
        app.on_key(ctrl('s')).unwrap();

        let save = save_mode(&app);
        assert_eq!(save.step, SaveStep::Command);
        assert_eq!(save.command, "kics scan -p .");

        app.on_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(save_mode(&app).step, SaveStep::Purpose);

        typed(&mut app, "Scan this project");
        app.on_key(key(KeyCode::Enter)).unwrap();

        assert!(matches!(app.mode(), Mode::Browse));
        assert!(app.status().unwrap().contains("user.kics-scan"));
        assert!(app.changed());

        // Saving reloads from disk, so the entry is now in the ranked list and
        // sitting under the cursor ready to be inserted.
        let saved = app.selected_row().unwrap();
        assert_eq!(saved.entry.id, "user.kics-scan");
        assert_eq!(saved.cmd, "kics scan -p .");
        assert_eq!(saved.entry.desc, "Scan this project");
        assert_eq!(saved.entry.tags, ["kics", "scan"]);

        let _ = std::fs::remove_file(&app.library);
    }

    #[test]
    fn hashtags_in_the_answer_become_tags() {
        let mut app = sample();
        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(&mut app, "Scan this project #security");

        assert_eq!(app.save_tags(), ["security", "kics", "scan"]);
        app.on_key(key(KeyCode::Enter)).unwrap();

        let saved = app.selected_row().unwrap();
        assert_eq!(saved.entry.desc, "Scan this project");
        assert_eq!(saved.entry.tags, ["security", "kics", "scan"]);

        let _ = std::fs::remove_file(&app.library);
    }

    #[test]
    fn saving_refuses_an_entry_nobody_could_find_later() {
        let mut app = sample();
        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(&mut app, "#only-tags");
        app.on_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(save_mode(&app).step, SaveStep::Purpose);
        assert!(app.status().unwrap().contains("what it is for"));
        assert!(!app.library.exists(), "an unfindable entry was written");
    }

    #[test]
    fn going_on_without_a_command_is_refused() {
        let mut app = app_with_history(Vec::new(), Vec::new());
        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(save_mode(&app).step, SaveStep::Command);
        assert_eq!(app.status(), Some(NO_COMMAND));
    }

    /// The shell having nothing to offer is not an error. The line is still
    /// there to be typed into.
    #[test]
    fn an_empty_history_still_opens_on_an_empty_line() {
        let mut app = app_with_history(
            vec![entry("git.log", "git log", "Show history")],
            Vec::new(),
        );
        app.on_key(ctrl('s')).unwrap();

        let save = save_mode(&app);
        assert_eq!(save.command, "");
        assert_eq!(save.recalled, None);
        assert!(app.status().is_none());

        app.on_key(key(KeyCode::Up)).unwrap();
        assert_eq!(
            save_mode(&app).command,
            "",
            "up found history that is not there"
        );
    }

    /// The way a shell's own prompt behaves: up walks back, down walks
    /// forward, and down past the newest leaves an empty line.
    #[test]
    fn up_and_down_walk_the_history_like_a_shell() {
        let mut app = history_sample();
        app.on_key(ctrl('s')).unwrap();
        assert_eq!(save_mode(&app).command, "kics scan -p .");

        app.on_key(key(KeyCode::Up)).unwrap();
        assert_eq!(save_mode(&app).command, "docker compose up -d");
        app.on_key(key(KeyCode::Up)).unwrap();
        assert_eq!(save_mode(&app).command, "git log --oneline");
        app.on_key(key(KeyCode::Up)).unwrap();
        assert_eq!(
            save_mode(&app).command,
            "git log --oneline",
            "ran off the end"
        );

        app.on_key(key(KeyCode::Down)).unwrap();
        app.on_key(key(KeyCode::Down)).unwrap();
        assert_eq!(save_mode(&app).command, "kics scan -p .");

        app.on_key(key(KeyCode::Down)).unwrap();
        let save = save_mode(&app);
        assert_eq!(save.command, "");
        assert_eq!(save.recalled, None);

        app.on_key(key(KeyCode::Up)).unwrap();
        assert_eq!(save_mode(&app).command, "kics scan -p .");
    }

    #[test]
    fn a_recalled_command_can_be_edited_before_it_is_saved() {
        let mut app = history_sample();
        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Up)).unwrap();
        for _ in 0.."-d".len() {
            app.on_key(key(KeyCode::Backspace)).unwrap();
        }
        typed(&mut app, "--build");

        assert_eq!(save_mode(&app).command, "docker compose up --build");
    }

    #[test]
    fn a_command_already_in_the_library_is_recognised() {
        let app = history_sample();
        assert!(app.is_saved("git log --oneline"));
        assert!(app.is_saved("  git log --oneline "));
        assert!(!app.is_saved("docker compose up -d"));
    }

    /// Up on the second line goes back to the first rather than into the
    /// history, and whatever was typed as the answer is kept.
    #[test]
    fn up_from_the_question_returns_to_the_command() {
        let mut app = history_sample();
        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(&mut app, "Scan");
        app.on_key(key(KeyCode::Up)).unwrap();

        let save = save_mode(&app);
        assert_eq!(save.step, SaveStep::Command);
        assert_eq!(save.command, "kics scan -p .");
        assert_eq!(save.purpose, "Scan");
    }

    #[test]
    fn escape_leaves_without_saving() {
        let mut app = history_sample();
        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(&mut app, "Scan this project");
        app.on_key(key(KeyCode::Esc)).unwrap();

        assert!(matches!(app.mode(), Mode::Browse));
        assert!(!app.library.exists());
        assert!(!app.changed());
    }

    #[test]
    fn a_repeated_or_blank_history_entry_is_dropped() {
        let prepared = prepare_history(vec![
            "  git status  ".to_string(),
            "   ".to_string(),
            "cd ..".to_string(),
            "git status".to_string(),
        ]);

        assert_eq!(
            prepared,
            vec!["git status".to_string(), "cd ..".to_string()]
        );
    }

    #[test]
    fn the_history_is_capped() {
        let raw: Vec<String> = (0..HISTORY_LIMIT + 10)
            .map(|n| format!("cmd {n}"))
            .collect();
        assert_eq!(prepare_history(raw).len(), HISTORY_LIMIT);
    }

    #[test]
    fn pinning_floats_an_entry_to_the_top() {
        let mut app = sample();
        typed(&mut app, "docker");
        app.on_key(ctrl('p')).unwrap();

        app.on_key(ctrl('u')).unwrap();
        assert_eq!(app.selected_row().unwrap().entry.id, "docker.ps");
        assert!(app.selected_row().unwrap().pinned);
    }

    #[test]
    fn pinning_twice_unpins() {
        let mut app = sample();
        app.on_key(ctrl('p')).unwrap();
        assert!(app.selected_row().unwrap().pinned);
        app.on_key(ctrl('p')).unwrap();
        assert!(!app.selected_row().unwrap().pinned);
    }

    fn save_one(app: &mut App, description: &str) {
        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(app, description);
        app.on_key(key(KeyCode::Enter)).unwrap();
    }

    #[test]
    fn editing_opens_on_what_the_entry_already_says() {
        let mut app = sample();
        let selected = app.selected_row().unwrap();
        let id = selected.entry.id.clone();
        let cmd = selected.cmd.to_string();
        let desc = selected.entry.desc.clone();
        app.on_key(ctrl('e')).unwrap();

        let Mode::Edit { form, .. } = app.mode() else {
            panic!("expected the edit form");
        };
        assert!(form.title.contains(&id), "the title never names the entry");
        assert_eq!(form.fields[0].value, cmd);
        assert_eq!(form.fields[1].value, desc);
    }

    #[test]
    fn editing_rewrites_the_entry_and_leaves_it_selected() {
        let mut app = app_with(vec![Entry {
            layer: Layer::User,
            ..entry("user.kics", "kics scan -p .", "Scan this project")
        }]);
        definitions::append(
            &app.library,
            &NewEntry {
                id: "user.kics".to_string(),
                cmd: CommandBody::Shared("kics scan -p .".to_string()),
                desc: "Scan this project".to_string(),
                tags: Vec::new(),
                params: BTreeMap::new(),
                danger: false,
            },
        )
        .unwrap();

        app.on_key(ctrl('e')).unwrap();
        typed(&mut app, " --report-formats json");
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();

        assert!(matches!(app.mode(), Mode::Browse));
        assert_eq!(app.status(), Some("Updated user.kics"));

        let edited = app.selected_row().unwrap();
        assert_eq!(edited.entry.id, "user.kics");
        assert_eq!(edited.cmd, "kics scan -p . --report-formats json");

        let _ = std::fs::remove_file(&app.library);
    }

    /// A builtin cannot be rewritten inside the binary, so the edit lands in the
    /// user's own library under the same id and shadows it from there.
    #[test]
    fn editing_a_builtin_writes_an_override_the_user_owns() {
        let mut app = app_with(vec![entry("git.log", "git log --oneline", "Show history")]);

        app.on_key(ctrl('e')).unwrap();
        typed(&mut app, " --graph");
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();

        assert!(app.status().unwrap().contains("overriding the builtin"));

        let text = std::fs::read_to_string(&app.library).unwrap();
        assert!(text.contains("id: git.log"), "wrote {text:?}");
        assert!(text.contains("--graph"), "wrote {text:?}");

        let _ = std::fs::remove_file(&app.library);
    }

    /// Three fields is two keystrokes of nothing on the way to the one that
    /// matters, so the chord submits from wherever the cursor happens to be.
    #[test]
    fn ctrl_s_submits_the_edit_form_from_any_field() {
        let mut app = app_with(vec![entry("git.log", "git log --oneline", "Show history")]);

        app.on_key(ctrl('e')).unwrap();
        typed(&mut app, " --graph");
        app.on_key(ctrl('s')).unwrap();

        assert!(matches!(app.mode(), Mode::Browse));
        assert_eq!(app.selected_row().unwrap().cmd, "git log --oneline --graph");

        let _ = std::fs::remove_file(&app.library);
    }

    #[test]
    fn ctrl_s_saves_from_the_answer_line() {
        let mut app = sample();

        app.on_key(ctrl('s')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        typed(&mut app, "Scan this project");
        app.on_key(ctrl('s')).unwrap();

        assert!(matches!(app.mode(), Mode::Browse));
        assert!(app.status().unwrap().contains("user.kics-scan"));

        let _ = std::fs::remove_file(&app.library);
    }

    /// Submitting early must not skip the check that the entry is findable.
    #[test]
    fn the_chord_still_refuses_an_entry_with_no_description() {
        let mut app = sample();
        app.on_key(ctrl('s')).unwrap();
        app.on_key(ctrl('s')).unwrap();

        assert_eq!(save_mode(&app).step, SaveStep::Purpose);
        assert!(app.status().unwrap().contains("what it is for"));
    }

    #[test]
    fn editing_refuses_an_entry_nobody_could_find_later() {
        let mut app = sample();
        app.on_key(ctrl('e')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(ctrl('u')).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Enter)).unwrap();

        assert!(matches!(app.mode(), Mode::Edit { .. }));
        assert!(app.status().unwrap().contains("description"));
    }

    #[test]
    fn leaving_the_edit_form_changes_nothing() {
        let mut app = sample();
        app.on_key(ctrl('e')).unwrap();
        typed(&mut app, " --graph");
        app.on_key(key(KeyCode::Esc)).unwrap();

        assert!(matches!(app.mode(), Mode::Browse));
        assert!(
            !app.library.exists(),
            "an abandoned edit still wrote a file"
        );
    }

    #[test]
    fn removing_asks_before_it_does_anything() {
        let mut app = sample();
        save_one(&mut app, "Scan this project");

        app.on_key(ctrl('x')).unwrap();

        assert!(app.status().unwrap().contains("ctrl+x again"));
        assert!(app.rows().any(|row| row.entry.id == "user.kics-scan"));

        let _ = std::fs::remove_file(&app.library);
    }

    #[test]
    fn confirming_takes_the_entry_out_of_the_library() {
        let mut app = sample();
        save_one(&mut app, "Scan this project");

        app.on_key(ctrl('x')).unwrap();
        app.on_key(ctrl('x')).unwrap();

        assert!(!app.rows().any(|row| row.entry.id == "user.kics-scan"));
        let library = std::fs::read_to_string(&app.library).unwrap();
        assert!(!library.contains("user.kics-scan"), "left {library:?}");

        let _ = std::fs::remove_file(&app.library);
    }

    #[test]
    fn any_other_key_stands_the_confirmation_down() {
        let mut app = sample();
        save_one(&mut app, "Scan this project");

        app.on_key(ctrl('x')).unwrap();
        app.on_key(key(KeyCode::Down)).unwrap();
        app.on_key(key(KeyCode::Up)).unwrap();
        app.on_key(ctrl('x')).unwrap();

        // Back to asking rather than removing.
        assert!(app.status().unwrap().contains("ctrl+x again"));
        assert!(app.rows().any(|row| row.entry.id == "user.kics-scan"));

        let _ = std::fs::remove_file(&app.library);
    }

    /// A builtin lives inside the binary, so removing it means recording that it
    /// should stop appearing.
    #[test]
    fn removing_a_builtin_disables_it_instead() {
        let mut app = sample();
        let id = app.selected_row().unwrap().entry.id.clone();

        app.on_key(ctrl('x')).unwrap();
        app.on_key(ctrl('x')).unwrap();

        assert!(app.status().unwrap().contains("disabled"));
        let library = std::fs::read_to_string(&app.library).unwrap();
        assert!(library.contains("disabled:"), "wrote {library:?}");
        assert!(library.contains(&id), "wrote {library:?}");

        let _ = std::fs::remove_file(&app.library);
    }

    #[test]
    fn removing_forgets_what_was_remembered_about_the_entry() {
        let mut app = sample();
        save_one(&mut app, "Scan this project");
        let id = app.selected_row().unwrap().entry.id.clone();

        app.on_key(ctrl('x')).unwrap();
        app.on_key(ctrl('x')).unwrap();

        assert!(!app.stats.scores(0).unwrap().contains_key(&id));

        let _ = std::fs::remove_file(&app.library);
    }

    #[test]
    fn a_query_matching_nothing_leaves_the_picker_usable() {
        let mut app = sample();
        typed(&mut app, "zzzzz");
        assert_eq!(app.matches(), 0);
        assert!(app.selected_row().is_none());
        assert_eq!(app.on_key(key(KeyCode::Enter)).unwrap(), None);
        app.on_key(key(KeyCode::Down)).unwrap();
        assert_eq!(app.selected(), 0);
    }
}
