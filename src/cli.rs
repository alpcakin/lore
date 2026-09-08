//! Command line surface.

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

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
    List,
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

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Init { .. } => bail!("`init` is not implemented yet"),
            Command::Setup => bail!("`setup` is not implemented yet"),
            Command::Uninstall => bail!("`uninstall` is not implemented yet"),
            Command::Pick { .. } => bail!("`pick` is not implemented yet"),
            Command::Save { .. } => bail!("`save` is not implemented yet"),
            Command::List => bail!("`list` is not implemented yet"),
        }
    }
}
