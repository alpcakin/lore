//! Command line surface.

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

use crate::model::{Entry, ShellFamily};
use crate::store::stats::{self, Stats};
use crate::store::{self, definitions};
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
    Setup,

    /// Remove the shell integration from the active shell profile.
    Uninstall,

    /// Open the picker and print the selected command to stdout.
    Pick {
        /// Shell the picker was invoked from, used to select command variants.
        #[arg(long)]
        shell: Option<Shell>,

        /// Command most recently run in the calling shell, offered for saving.
        #[arg(long)]
        last: Option<String>,
    },

    /// Save a command to the user library.
    Save { command: String },

    /// List commands without opening the picker.
    List {
        /// Shell to resolve command variants for.
        #[arg(long)]
        shell: Option<Shell>,
    },
}

/// Shells that lore ships a keybinding integration for.
// Renaming `PowerShell` to satisfy `enum_variant_names` would misrepresent
// Windows PowerShell 5.1, which is a supported target alongside PowerShell 7.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
}

impl From<Shell> for ShellFamily {
    fn from(shell: Shell) -> Self {
        match shell {
            Shell::Bash | Shell::Zsh | Shell::Fish => ShellFamily::Posix,
            Shell::PowerShell => ShellFamily::PowerShell,
        }
    }
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Init { .. } => bail!("`init` is not implemented yet"),
            Command::Setup => bail!("`setup` is not implemented yet"),
            Command::Uninstall => bail!("`uninstall` is not implemented yet"),
            Command::Pick { shell, last } => pick(family(shell), last),
            Command::Save { .. } => bail!("`save` is not implemented yet"),
            Command::List { shell } => list(family(shell)),
        }
    }
}

/// Falls back to the dialect this build most likely runs under until shell
/// detection lands with the integration snippets.
fn family(shell: Option<Shell>) -> ShellFamily {
    match shell {
        Some(shell) => shell.into(),
        None if cfg!(windows) => ShellFamily::PowerShell,
        None => ShellFamily::Posix,
    }
}

/// Opens the picker and writes the chosen command to stdout.
///
/// Only the command goes to stdout: the shell integration captures it and puts
/// it in the prompt. Pressing enter on it stays the user's decision.
fn pick(family: ShellFamily, last: Option<String>) -> Result<()> {
    let library = store::user_library()?;
    let entries = definitions::load(Some(&library))?;
    let stats = Stats::open(&store::stats_database()?)?;

    let app = App::new(entries, family, stats, library, last, stats::now())?;
    if let Outcome::Insert(command) = crate::tui::run(app)? {
        println!("{command}");
    }

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
