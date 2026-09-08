//! Command line surface.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::model::{Entry, ShellFamily};
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
    Init { shell: Shell },

    /// Install the shell integration into the active shell profile.
    Setup {
        /// Shell to set up. Detected from the environment when omitted.
        #[arg(long)]
        shell: Option<Shell>,

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

        /// Command most recently run in the calling shell, offered for saving.
        // A history entry can begin with a hyphen, and clap would otherwise
        // reject it as an unknown flag. Every snippet passes this last, so
        // nothing else can be swallowed by it.
        #[arg(long, allow_hyphen_values = true)]
        last: Option<String>,
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
            Command::Init { shell } => {
                print!("{}", shell::snippet(shell));
                Ok(())
            }
            Command::Setup { shell, yes } => shell::install(resolve(shell)?, yes),
            Command::Uninstall { shell } => shell::uninstall(resolve(shell)?),
            Command::Pick { shell, last } => pick(family(shell), last),
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
fn pick(family: ShellFamily, last: Option<String>) -> Result<()> {
    let last = last.filter(|command| !command.trim().is_empty());
    let library = store::user_library()?;
    let entries = definitions::load(Some(&library))?;
    let stats = Stats::open(&store::stats_database()?)?;

    let app = App::new(entries, family, stats, library, last, stats::now())?;
    if let Outcome::Insert(command) = crate::tui::run(app)? {
        println!("{command}");
    }

    Ok(())
}

fn save(command: String, desc: String, tags: Option<String>) -> Result<()> {
    let library = store::user_library()?;
    let taken: BTreeSet<String> = definitions::load(Some(&library))?
        .into_iter()
        .map(|entry| entry.id)
        .collect();

    let entry = NewEntry {
        id: definitions::suggest_id(&command, &taken),
        cmd: command,
        desc,
        tags: tags
            .unwrap_or_default()
            .split(',')
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect(),
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

    fn last_of(arguments: &[&str]) -> Option<String> {
        match Cli::try_parse_from(arguments)
            .expect("arguments should parse")
            .command
        {
            Command::Pick { last, .. } => last,
            _ => panic!("expected pick"),
        }
    }

    #[test]
    fn a_previous_command_beginning_with_a_hyphen_is_still_a_value() {
        assert_eq!(
            last_of(&["lore", "pick", "--shell", "bash", "--last", "-Verbose"]),
            Some("-Verbose".to_string())
        );
    }

    #[test]
    fn omitting_the_previous_command_is_allowed() {
        assert_eq!(last_of(&["lore", "pick", "--shell", "powershell"]), None);
    }
}
