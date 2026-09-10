//! The key that opens the picker, written once and translated per shell.
//!
//! Every shell spells a chord differently, and `init` is not allowed to read a
//! config file, so the chord travels through the init line as a canonical
//! `ctrl-g` and each snippet receives whatever its own binding syntax expects.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::shell::Shell;

/// The chord used when `--key` is not given.
pub const DEFAULT: &str = "ctrl-g";

/// Control keys the terminal itself consumes or that carry a meaning nothing
/// should take over. Binding `ctrl-m` costs the user the enter key.
const RESERVED: &[(char, &str)] = &[
    ('c', "interrupts the running command"),
    ('d', "ends input"),
    ('i', "is the tab key"),
    ('j', "is a line feed"),
    ('m', "is the enter key"),
    ('q', "resumes a stopped terminal"),
    ('s', "stops terminal output"),
    ('z', "suspends the running command"),
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChordError {
    #[error("write the key as ctrl-<letter> or alt-<letter>, such as {DEFAULT}")]
    Shape,

    #[error("{0} is not a modifier lore can bind, use ctrl or alt")]
    Modifier(String),

    #[error("{0} is not a single letter")]
    Key(String),

    #[error("ctrl-{0} cannot be bound because it {1}")]
    Reserved(char, &'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Alt,
}

/// A keybinding that opens the picker.
///
/// Deliberately narrow. A letter with control or alt is the only combination
/// all four shells spell unambiguously and every terminal actually transmits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    modifier: Modifier,
    letter: char,
}

impl Chord {
    /// The chord in the binding syntax `shell` expects.
    pub fn render(self, shell: Shell) -> String {
        let letter = self.letter;
        match (shell, self.modifier) {
            (Shell::Bash, Modifier::Ctrl) => format!(r"\C-{letter}"),
            (Shell::Bash, Modifier::Alt) => format!(r"\M-{letter}"),
            // A control key is written as the letter it is typed with, which is
            // the upper case one; alt is the escape prefix.
            (Shell::Zsh, Modifier::Ctrl) => format!("^{}", letter.to_ascii_uppercase()),
            (Shell::Zsh, Modifier::Alt) => format!("^[{letter}"),
            (Shell::Fish, Modifier::Ctrl) => format!(r"\c{letter}"),
            (Shell::Fish, Modifier::Alt) => format!(r"\e{letter}"),
            (Shell::PowerShell, Modifier::Ctrl) => format!("Ctrl+{letter}"),
            (Shell::PowerShell, Modifier::Alt) => format!("Alt+{letter}"),
        }
    }

    /// How to name the chord when telling the user which key to press.
    pub fn spoken(self) -> String {
        let modifier = match self.modifier {
            Modifier::Ctrl => "ctrl",
            Modifier::Alt => "alt",
        };
        format!("{modifier}+{}", self.letter)
    }

    pub fn is_default(self) -> bool {
        self == Self::default()
    }
}

impl Default for Chord {
    fn default() -> Self {
        DEFAULT.parse().expect("the default chord should parse")
    }
}

impl FromStr for Chord {
    type Err = ChordError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim().to_ascii_lowercase();
        let (modifier, key) = text.split_once('-').ok_or(ChordError::Shape)?;

        let modifier = match modifier {
            "ctrl" | "control" => Modifier::Ctrl,
            "alt" | "meta" => Modifier::Alt,
            other => return Err(ChordError::Modifier(other.to_string())),
        };

        let mut letters = key.chars();
        let letter = match (letters.next(), letters.next()) {
            (Some(letter), None) if letter.is_ascii_alphabetic() => letter,
            _ => return Err(ChordError::Key(key.to_string())),
        };

        if modifier == Modifier::Ctrl
            && let Some((_, why)) = RESERVED.iter().find(|(reserved, _)| *reserved == letter)
        {
            return Err(ChordError::Reserved(letter, why));
        }

        Ok(Self { modifier, letter })
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let modifier = match self.modifier {
            Modifier::Ctrl => "ctrl",
            Modifier::Alt => "alt",
        };
        write!(f, "{modifier}-{}", self.letter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        text.parse().expect("should parse")
    }

    #[test]
    fn the_default_is_the_chord_the_documentation_promises() {
        assert_eq!(Chord::default().to_string(), "ctrl-g");
        assert!(chord("ctrl-g").is_default());
        assert!(!chord("alt-r").is_default());
    }

    #[test]
    fn parsing_is_case_insensitive_and_round_trips() {
        for text in ["ctrl-g", "CTRL-G", " Ctrl-g "] {
            assert_eq!(chord(text).to_string(), "ctrl-g");
        }
        assert_eq!(chord("Alt-R").to_string(), "alt-r");
        assert_eq!(chord("control-p").to_string(), "ctrl-p");
        assert_eq!(chord("meta-p").to_string(), "alt-p");
    }

    #[test]
    fn every_shell_gets_its_own_spelling() {
        let ctrl = chord("ctrl-g");
        assert_eq!(ctrl.render(Shell::Bash), r"\C-g");
        assert_eq!(ctrl.render(Shell::Zsh), "^G");
        assert_eq!(ctrl.render(Shell::Fish), r"\cg");
        assert_eq!(ctrl.render(Shell::PowerShell), "Ctrl+g");

        let alt = chord("alt-r");
        assert_eq!(alt.render(Shell::Bash), r"\M-r");
        assert_eq!(alt.render(Shell::Zsh), "^[r");
        assert_eq!(alt.render(Shell::Fish), r"\er");
        assert_eq!(alt.render(Shell::PowerShell), "Alt+r");
    }

    #[test]
    fn a_chord_without_a_modifier_is_refused() {
        assert_eq!("g".parse::<Chord>(), Err(ChordError::Shape));
        assert_eq!("".parse::<Chord>(), Err(ChordError::Shape));
    }

    #[test]
    fn an_unbindable_modifier_is_refused() {
        assert!(matches!(
            "shift-g".parse::<Chord>(),
            Err(ChordError::Modifier(_))
        ));
        assert!(matches!(
            "ctrl-shift-g".parse::<Chord>(),
            Err(ChordError::Key(_))
        ));
    }

    #[test]
    fn a_key_that_is_not_one_letter_is_refused() {
        for text in ["ctrl-", "ctrl-space", "ctrl-1", "alt-f4"] {
            assert!(
                matches!(text.parse::<Chord>(), Err(ChordError::Key(_))),
                "{text}"
            );
        }
    }

    /// Binding one of these would cost the user a key the terminal needs more
    /// than it needs the picker.
    #[test]
    fn control_keys_the_terminal_owns_are_refused() {
        for (letter, _) in RESERVED {
            let text = format!("ctrl-{letter}");
            assert!(
                matches!(text.parse::<Chord>(), Err(ChordError::Reserved(..))),
                "{text}"
            );
        }
        // Alt does not go through the terminal's control character table, so
        // the same letters are free there.
        assert!("alt-c".parse::<Chord>().is_ok());
    }

    #[test]
    fn the_spoken_form_reads_like_a_key_to_press() {
        assert_eq!(chord("ctrl-g").spoken(), "ctrl+g");
        assert_eq!(chord("alt-r").spoken(), "alt+r");
    }
}
