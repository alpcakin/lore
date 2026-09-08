//! A small multi-field text form, shared by the parameter and save screens.

use crate::search;

pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focused: usize,
    /// Set while a value is being chosen from the focused field's list.
    pub picking: Option<Picker>,
}

pub struct Field {
    pub label: String,
    pub hint: Option<String>,
    pub value: String,
    /// Values on offer for this field, best first.
    pub choices: Vec<Choice>,
}

/// One offered value, with an optional note drawn beside it.
pub struct Choice {
    pub value: String,
    pub note: Option<String>,
}

/// Choosing a value from the focused field's list.
///
/// The field is left alone until the choice is accepted, so backing out of the
/// list has nothing to undo.
pub struct Picker {
    pub title: String,
    pub filter: String,
    pub selected: usize,
}

impl Form {
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            focused: 0,
            picking: None,
        }
    }

    pub fn focused(&mut self) -> &mut Field {
        &mut self.fields[self.focused]
    }

    /// Moves to the next field, reporting whether the form was already on the
    /// last one and is therefore ready to submit.
    pub fn advance(&mut self) -> bool {
        if self.focused + 1 == self.fields.len() {
            return true;
        }
        self.focused += 1;
        false
    }

    pub fn retreat(&mut self) {
        self.focused = self.focused.saturating_sub(1);
    }

    pub fn insert(&mut self, character: char) {
        self.focused().value.push(character);
    }

    pub fn backspace(&mut self) {
        self.focused().value.pop();
    }

    pub fn clear(&mut self) {
        self.focused().value.clear();
    }

    pub fn value(&self, index: usize) -> &str {
        self.fields[index].value.trim()
    }

    /// Opens the focused field's list, reporting whether it had one.
    ///
    /// It opens on whatever the field already holds, so the list says where the
    /// current value sits among the rest rather than starting somewhere else.
    pub fn open_picker(&mut self, title: impl Into<String>) -> bool {
        let field = &self.fields[self.focused];
        if field.choices.is_empty() {
            return false;
        }

        let selected = field
            .choices
            .iter()
            .position(|choice| choice.value == field.value)
            .unwrap_or(0);

        self.picking = Some(Picker {
            title: title.into(),
            filter: String::new(),
            selected,
        });
        true
    }

    /// The choices the filter leaves on offer.
    pub fn visible(&self) -> Vec<&Choice> {
        let Some(picker) = &self.picking else {
            return Vec::new();
        };

        self.fields[self.focused]
            .choices
            .iter()
            .filter(|choice| search::matches(&choice.value, &picker.filter))
            .collect()
    }

    pub fn move_pick(&mut self, delta: isize) {
        let count = self.visible().len();
        let Some(picker) = &mut self.picking else {
            return;
        };

        if count == 0 {
            picker.selected = 0;
            return;
        }
        let target = picker.selected as isize + delta;
        picker.selected = target.clamp(0, count as isize - 1) as usize;
    }

    pub fn filter_insert(&mut self, character: char) {
        if let Some(picker) = &mut self.picking {
            picker.filter.push(character);
        }
        self.move_pick(0);
    }

    pub fn filter_backspace(&mut self) {
        if let Some(picker) = &mut self.picking {
            picker.filter.pop();
        }
        self.move_pick(0);
    }

    pub fn filter_clear(&mut self) {
        if let Some(picker) = &mut self.picking {
            picker.filter.clear();
        }
        self.move_pick(0);
    }

    /// Puts the highlighted choice into the field and closes the list.
    pub fn accept_pick(&mut self) {
        let selected = self.picking.as_ref().map_or(0, |picker| picker.selected);
        let Some(value) = self
            .visible()
            .get(selected)
            .map(|choice| choice.value.clone())
        else {
            return;
        };

        self.focused().value = value;
        self.picking = None;
    }

    pub fn cancel_pick(&mut self) {
        self.picking = None;
    }
}

impl Field {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hint: None,
            value: value.into(),
            choices: Vec::new(),
        }
    }

    pub fn with_hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
        self
    }

    pub fn with_choices(mut self, choices: Vec<Choice>) -> Self {
        self.choices = choices;
        self
    }
}

impl Choice {
    pub fn new(value: impl Into<String>, note: Option<String>) -> Self {
        Self {
            value: value.into(),
            note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::new(
            "test",
            vec![Field::new("first", ""), Field::new("second", "seed")],
        )
    }

    fn with_choices() -> Form {
        Form::new(
            "test",
            vec![
                Field::new("command", "docker ps -a").with_choices(vec![
                    Choice::new("git log --oneline", None),
                    Choice::new("docker ps -a", Some("already saved".to_string())),
                    Choice::new("cargo test", None),
                ]),
                Field::new("description", ""),
            ],
        )
    }

    #[test]
    fn advancing_stops_on_the_last_field() {
        let mut form = form();
        assert!(!form.advance());
        assert_eq!(form.focused, 1);
        assert!(form.advance());
        assert_eq!(form.focused, 1);
    }

    #[test]
    fn retreating_stops_on_the_first_field() {
        let mut form = form();
        form.advance();
        form.retreat();
        form.retreat();
        assert_eq!(form.focused, 0);
    }

    #[test]
    fn editing_touches_only_the_focused_field() {
        let mut form = form();
        form.insert('a');
        form.insert('b');
        form.backspace();
        assert_eq!(form.value(0), "a");
        assert_eq!(form.value(1), "seed");
    }

    #[test]
    fn clearing_empties_the_focused_field() {
        let mut form = form();
        form.advance();
        form.clear();
        assert_eq!(form.value(1), "");
    }

    #[test]
    fn a_field_with_no_choices_has_no_list_to_open() {
        let mut form = form();
        assert!(!form.open_picker("Recent commands"));
        assert!(form.picking.is_none());
    }

    /// The list says where the value in the field sits among the rest, so it
    /// opens on that row rather than at the top.
    #[test]
    fn the_list_opens_on_the_value_the_field_already_holds() {
        let mut form = with_choices();
        assert!(form.open_picker("Recent commands"));
        assert_eq!(form.picking.as_ref().unwrap().selected, 1);
    }

    #[test]
    fn accepting_writes_the_highlighted_choice_into_the_field() {
        let mut form = with_choices();
        form.open_picker("Recent commands");
        form.move_pick(1);
        form.accept_pick();

        assert_eq!(form.value(0), "cargo test");
        assert!(form.picking.is_none());
    }

    #[test]
    fn cancelling_leaves_the_field_as_it_was() {
        let mut form = with_choices();
        form.open_picker("Recent commands");
        form.move_pick(-1);
        form.cancel_pick();

        assert_eq!(form.value(0), "docker ps -a");
        assert!(form.picking.is_none());
    }

    #[test]
    fn the_filter_narrows_the_list() {
        let mut form = with_choices();
        form.open_picker("Recent commands");
        for character in "car".chars() {
            form.filter_insert(character);
        }

        assert_eq!(form.visible().len(), 1);
        form.accept_pick();
        assert_eq!(form.value(0), "cargo test");
    }

    /// Narrowing must not leave the cursor past the end of what is left.
    #[test]
    fn a_filter_matching_nothing_leaves_the_list_usable() {
        let mut form = with_choices();
        form.open_picker("Recent commands");
        for character in "zzzz".chars() {
            form.filter_insert(character);
        }

        assert!(form.visible().is_empty());
        assert_eq!(form.picking.as_ref().unwrap().selected, 0);

        form.accept_pick();
        assert!(form.picking.is_some());
        assert_eq!(form.value(0), "docker ps -a");

        form.filter_clear();
        assert_eq!(form.visible().len(), 3);
    }

    #[test]
    fn the_cursor_stays_inside_the_list() {
        let mut form = with_choices();
        form.open_picker("Recent commands");

        form.move_pick(10);
        assert_eq!(form.picking.as_ref().unwrap().selected, 2);
        form.move_pick(-10);
        assert_eq!(form.picking.as_ref().unwrap().selected, 0);
    }
}
