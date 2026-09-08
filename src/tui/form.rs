//! A small multi-field text form, shared by the parameter and save screens.

pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focused: usize,
}

pub struct Field {
    pub label: String,
    pub hint: Option<String>,
    pub value: String,
}

impl Form {
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            focused: 0,
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
}

impl Field {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hint: None,
            value: value.into(),
        }
    }

    pub fn with_hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
        self
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
}
