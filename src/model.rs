//! Types describing a command library.

use std::collections::BTreeMap;

use serde::Deserialize;

/// One command definition file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub version: u32,

    #[serde(default)]
    pub commands: Vec<Entry>,

    /// Glob patterns matched against entry ids, used to hide entries from
    /// earlier layers without redefining them.
    #[serde(default)]
    pub disabled: Vec<String>,
}

/// A single command a user can pick.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub cmd: CommandBody,
    pub desc: String,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub params: BTreeMap<String, ParamSpec>,

    /// Marks a destructive command so the picker can warn before it is inserted.
    #[serde(default)]
    pub danger: bool,

    #[serde(skip)]
    pub layer: Layer,
}

/// A command string, optionally specialised per shell family.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CommandBody {
    Shared(String),
    PerShell(BTreeMap<ShellFamily, String>),
}

/// Optional metadata for a placeholder used in a command.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamSpec {
    #[serde(default)]
    pub desc: Option<String>,

    /// Command whose output supplies selectable values for this placeholder.
    /// Reserved by the schema; the current runtime ignores it and falls back to
    /// a free text prompt.
    #[serde(default)]
    pub from: Option<String>,
}

/// Where an entry came from. Later layers shadow earlier ones by id.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    #[default]
    Builtin,
    Project,
    User,
}

/// Shells that share a command dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellFamily {
    Posix,
    PowerShell,
}

impl Entry {
    /// The command text for `family`, or `None` when this entry has no variant
    /// for it and should be hidden from that shell.
    pub fn cmd_for(&self, family: ShellFamily) -> Option<&str> {
        match &self.cmd {
            CommandBody::Shared(cmd) => Some(cmd),
            CommandBody::PerShell(variants) => variants.get(&family).map(String::as_str),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(cmd: CommandBody) -> Entry {
        Entry {
            id: "test".into(),
            cmd,
            desc: "test".into(),
            tags: Vec::new(),
            params: BTreeMap::new(),
            danger: false,
            layer: Layer::Builtin,
        }
    }

    #[test]
    fn shared_command_serves_every_family() {
        let e = entry(CommandBody::Shared("git status".into()));
        assert_eq!(e.cmd_for(ShellFamily::Posix), Some("git status"));
        assert_eq!(e.cmd_for(ShellFamily::PowerShell), Some("git status"));
    }

    #[test]
    fn missing_variant_hides_entry_from_that_shell() {
        let e = entry(CommandBody::PerShell(BTreeMap::from([(
            ShellFamily::Posix,
            "ss -tulpn".into(),
        )])));
        assert_eq!(e.cmd_for(ShellFamily::Posix), Some("ss -tulpn"));
        assert_eq!(e.cmd_for(ShellFamily::PowerShell), None);
    }

    #[test]
    fn layers_order_from_weakest_to_strongest() {
        assert!(Layer::Builtin < Layer::Project);
        assert!(Layer::Project < Layer::User);
    }
}
