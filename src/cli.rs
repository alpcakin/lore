//! Command line surface.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::model::{CommandBody, Entry, ShellFamily};
use crate::shell::chord::{self, Chord};
use crate::shell::{self, Shell};
use crate::store::definitions::{self, NewEntry};
use crate::store::stats::{self, Stats};
use crate::store::{self};
use crate::tui::{App, Outcome};

/// A command library that lives in your shell.
#[derive(Parser)]
#[command(name = "lore", version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the shell integration snippet for the given shell.
    Init {
        shell: Shell,

        /// Key that opens the picker, written as ctrl-g or alt-r.
        #[arg(long, value_name = "CHORD", default_value = chord::DEFAULT)]
        key: Chord,
    },

    /// Install the shell integration into the active shell profile.
    Setup {
        /// Shell to set up. Detected from the environment when omitted.
        #[arg(long)]
        shell: Option<Shell>,

        /// Key that opens the picker, written as ctrl-g or alt-r.
        #[arg(long, value_name = "CHORD", default_value = chord::DEFAULT)]
        key: Chord,

        /// Do not ask before writing to a profile.
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Remove the shell integration from the active shell profile.
    Uninstall {
        /// Shell to clean up. Detected from the environment when omitted.
        #[arg(long)]
        shell: Option<Shell>,
    },

    /// Open the picker and print the selected command to stdout.
    Pick {
        /// Shell the picker was invoked from, used to select command variants.
        #[arg(long)]
        shell: Option<Shell>,

        /// File of the calling shell's recent commands, newest first, one a
        /// line.
        // A file rather than arguments. Windows hands a child one string and
        // lets it split its own arguments, so a command ending in a backslash
        // escapes the quote that was meant to close it and swallows whatever
        // came next. `cd C:\\projects\\` is enough to do it.
        #[arg(long)]
        history: Option<PathBuf>,
    },

    /// Save a command to the user library without opening the picker.
    Save {
        command: String,

        /// What the command is for. This is how you will find it again.
        #[arg(long)]
        desc: String,

        /// Comma separated keywords.
        #[arg(long)]
        tags: Option<String>,
    },

    /// List commands without opening the picker.
    List {
        /// Shell to resolve command variants for.
        #[arg(long)]
        shell: Option<Shell>,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Init { shell, key } => {
                print!("{}", shell::snippet(shell, key));
                Ok(())
            }
            Command::Setup { shell, key, yes } => shell::install(resolve(shell)?, key, yes),
            Command::Uninstall { shell } => shell::uninstall(resolve(shell)?),
            Command::Pick { shell, history } => pick(family(shell), history.as_deref()),
            Command::Save {
                command,
                desc,
                tags,
            } => save(command, desc, tags),
            Command::List { shell } => list(family(shell)),
        }
    }
}

fn resolve(shell: Option<Shell>) -> Result<Shell> {
    match shell.or_else(shell::detect) {
        Some(shell) => Ok(shell),
        None => bail!("could not tell which shell you are using, pass --shell"),
    }
}

fn family(shell: Option<Shell>) -> ShellFamily {
    shell
        .or_else(shell::detect)
        .map(ShellFamily::from)
        .unwrap_or(ShellFamily::Posix)
}

/// Opens the picker and writes the chosen command to stdout.
///
/// Only the command goes to stdout: the shell integration captures it and puts
/// it in the prompt. Pressing enter on it stays the user's decision.
fn pick(family: ShellFamily, history: Option<&Path>) -> Result<()> {
    let library = store::user_library()?;
    let entries = definitions::load(Some(&library))?;
    let stats = Stats::open(&store::stats_database()?)?;

    let history = read_history(history);
    let app = App::new(entries, family, stats, library, history, stats::now())?;
    if let Outcome::Insert(command) = crate::tui::run(app)? {
        println!("{command}");
    }

    Ok(())
}

/// The shell's recent commands, or nothing at all.
///
/// A history that cannot be read is not worth refusing to open the picker over:
/// everything else it does still works without one.
fn read_history(path: Option<&Path>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };

    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn save(command: String, desc: String, tags: Option<String>) -> Result<()> {
    let library = store::user_library()?;
    let taken: BTreeSet<String> = definitions::load(Some(&library))?
        .into_iter()
        .map(|entry| entry.id)
        .collect();

    let entry = NewEntry {
        id: definitions::suggest_id(&command, &taken),
        cmd: CommandBody::Shared(command),
        desc,
        tags: definitions::parse_tags(&tags.unwrap_or_default()),
        params: BTreeMap::new(),
        danger: false,
    };

    let stats = Stats::open(&store::stats_database()?)?;
    stats.record_new(&entry.id, stats::now())?;
    definitions::append(&library, &entry)?;

    println!("Saved as {} in {}", entry.id, library.display());
    Ok(())
}

fn list(family: ShellFamily) -> Result<()> {
    let library = store::user_library().ok();
    let entries = definitions::load(library.as_deref())?;

    for entry in entries.iter().filter(|e| e.cmd_for(family).is_some()) {
        print(entry, family);
    }

    Ok(())
}

fn print(entry: &Entry, family: ShellFamily) {
    let cmd = entry.cmd_for(family).expect("caller filtered on this");
    let danger = if entry.danger { "  [destructive]" } else { "" };

    println!("{}{danger}", entry.id);
    println!("  {}", entry.desc);
    println!("  {cmd}");

    for name in crate::params::names(cmd) {
        let desc = entry
            .params
            .get(&name)
            .and_then(|spec| spec.desc.as_deref())
            .unwrap_or("no description");
        println!("    <{name}>  {desc}");
    }

    if !entry.tags.is_empty() {
        println!("  tags: {}", entry.tags.join(", "));
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_of(arguments: &[&str]) -> Option<PathBuf> {
        match Cli::try_parse_from(arguments)
            .expect("arguments should parse")
            .command
        {
            Command::Pick { history, .. } => history,
            _ => panic!("expected pick"),
        }
    }

    #[test]
    fn omitting_the_history_is_allowed() {
        assert!(history_of(&["lore", "pick", "--shell", "powershell"]).is_none());
        assert!(read_history(None).is_empty());
    }

    /// The reason the history travels in a file. Every one of these survives
    /// being written to disk and read back, and none of them survives being
    /// rebuilt out of a Windows command line.
    #[test]
    fn a_history_file_carries_commands_an_argument_list_cannot() {
        let path = std::env::temp_dir().join(format!("lore-history-{}.txt", std::process::id()));
        let written = "cd C:\\projects\\\ngit commit -m \"fix the thing\"\n-Verbose\n";
        fs::write(&path, written).unwrap();

        assert_eq!(
            history_of(&[
                "lore",
                "pick",
                "--shell",
                "powershell",
                "--history",
                path.to_str().unwrap()
            ]),
            Some(path.clone())
        );
        assert_eq!(
            read_history(Some(&path)),
            vec![
                "cd C:\\projects\\".to_string(),
                "git commit -m \"fix the thing\"".to_string(),
                "-Verbose".to_string(),
            ]
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn an_unreadable_history_leaves_the_picker_openable() {
        assert!(read_history(Some(Path::new("no-such-file-anywhere"))).is_empty());
    }
}
