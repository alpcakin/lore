//! Command line surface.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::model::{CommandBody, Entry, Layer, ShellFamily};
use crate::shell::chord::{self, Chord};
use crate::shell::{self, Shell};
use crate::store::definitions::{self, NewEntry, Written};
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

        /// Print the cursor offset on a line of its own before the command.
        ///
        /// The shell integration passes this; running the picker by hand still
        /// gets nothing but the command.
        #[arg(long)]
        print_cursor: bool,

        /// File to write the result into instead of stdout.
        // The picker has to ask the terminal where the cursor is, and the
        // library it uses writes that question to stdout. A shell that captured
        // stdout to read the result would swallow the question, no answer would
        // come back, and the panel would never open. The result travels in a
        // file so stdout can stay attached to the terminal.
        #[arg(long)]
        output: Option<PathBuf>,

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

    /// Change a command already in the library.
    Edit {
        id: String,

        /// Shell whose command variant to replace. Detected when omitted.
        #[arg(long)]
        shell: Option<Shell>,

        #[arg(long)]
        cmd: Option<String>,

        #[arg(long)]
        desc: Option<String>,

        /// Comma separated keywords, replacing the ones already there.
        #[arg(long)]
        tags: Option<String>,
    },

    /// Take a command out of the library.
    #[command(alias = "remove")]
    Rm { id: String },

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
                let mut out = io::stdout().lock();
                out.write_all(shell::snippet(shell, key).as_bytes())?;
                out.flush()?;
                Ok(())
            }
            Command::Setup { shell, key, yes } => shell::install(resolve(shell)?, key, yes),
            Command::Uninstall { shell } => shell::uninstall(resolve(shell)?),
            Command::Pick {
                shell,
                print_cursor,
                output,
                history,
            } => pick(
                family(shell),
                print_cursor,
                output.as_deref(),
                history.as_deref(),
            ),
            Command::Save {
                command,
                desc,
                tags,
            } => save(command, desc, tags),
            Command::Edit {
                id,
                shell,
                cmd,
                desc,
                tags,
            } => edit(id, family(shell), cmd, desc, tags),
            Command::Rm { id } => remove(id),
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
/// Runs the picker and hands back the chosen command. Pressing enter on it
/// stays the user's decision.
///
/// The result goes to `output` when one is given and to stdout otherwise. The
/// shell integration always gives one: the picker asks the terminal where the
/// cursor is by writing to stdout, so a shell that captured stdout to read the
/// result would swallow the question and the panel would never open.
///
/// With `print_cursor` the offset comes first, on its own line, and the command
/// is everything after it. The offset leads so that the command stays the tail
/// and needs no parsing to recover.
fn pick(
    family: ShellFamily,
    print_cursor: bool,
    output: Option<&Path>,
    history: Option<&Path>,
) -> Result<()> {
    let library = store::user_library()?;
    let entries = definitions::load(Some(&library))?;
    let stats = Stats::open(&store::stats_database()?)?;

    let history = read_history(history);
    let app = App::new(entries, family, stats, library, history, stats::now())?;

    let Outcome::Insert { command, cursor } = crate::tui::run(app)? else {
        return Ok(());
    };

    let mut result = String::new();
    if print_cursor {
        let offset = cursor.unwrap_or(command.chars().count());
        result.push_str(&format!("{offset}\n"));
    }
    result.push_str(&command);
    result.push('\n');

    match output {
        Some(path) => fs::write(path, result)
            .with_context(|| format!("failed to write {}", path.display()))?,
        None => {
            let mut out = io::stdout().lock();
            out.write_all(result.as_bytes())?;
            out.flush()?;
        }
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

/// Applies the given changes to an entry, leaving every other field alone.
///
/// A builtin is written to the user's library under its own id rather than
/// changed inside the binary, which the loader turns into an override.
fn edit(
    id: String,
    family: ShellFamily,
    cmd: Option<String>,
    desc: Option<String>,
    tags: Option<String>,
) -> Result<()> {
    if cmd.is_none() && desc.is_none() && tags.is_none() {
        bail!("nothing to change, pass at least one of --cmd, --desc or --tags");
    }

    let library = store::user_library()?;
    let entries = definitions::load(Some(&library))?;
    let Some(entry) = entries.iter().find(|entry| entry.id == id) else {
        bail!("no command with the id {id}");
    };

    // Only the variant for this shell is replaced. The others were never named
    // and are none of this edit's business.
    let body = match (&entry.cmd, cmd) {
        (_, None) => entry.cmd.clone(),
        (CommandBody::Shared(_), Some(cmd)) => CommandBody::Shared(cmd),
        (CommandBody::PerShell(variants), Some(cmd)) => {
            let mut variants = variants.clone();
            variants.insert(family, cmd);
            CommandBody::PerShell(variants)
        }
    };

    let edited = NewEntry {
        id: id.clone(),
        cmd: body,
        desc: desc.unwrap_or_else(|| entry.desc.clone()),
        tags: tags
            .map(|tags| definitions::parse_tags(&tags))
            .unwrap_or_else(|| entry.tags.clone()),
        params: entry.params.clone(),
        danger: entry.danger,
    };

    match definitions::upsert(&library, &edited)? {
        Written::Replaced => println!("Updated {id} in {}", library.display()),
        Written::Appended => println!(
            "Saved {id} to {}, overriding the builtin",
            library.display()
        ),
    }

    Ok(())
}

/// Takes an entry out of the picker.
///
/// A builtin lives inside the binary and cannot be deleted, so it is added to
/// the user's disabled list instead. Either way it stops appearing, which is
/// what was asked for.
fn remove(id: String) -> Result<()> {
    let library = store::user_library()?;
    let entries = definitions::load(Some(&library))?;
    let Some(entry) = entries.iter().find(|entry| entry.id == id) else {
        bail!("no command with the id {id}");
    };

    if entry.layer == Layer::User {
        definitions::remove(&library, &id)?;
        println!("Removed {id} from {}", library.display());
    } else {
        definitions::disable(&library, &id)?;
        println!("Hid {id}, listed under disabled in {}", library.display());
    }

    Stats::open(&store::stats_database()?)?.forget(&id)?;
    Ok(())
}

/// Writes the library to stdout.
///
/// Through a writer that returns its errors rather than `println!`, which
/// panics when the reader goes away. `lore list | head` closes the pipe after
/// ten lines, and that has to end the listing quietly: see `main`.
fn list(family: ShellFamily) -> Result<()> {
    let library = store::user_library().ok();
    let entries = definitions::load(library.as_deref())?;

    let mut out = io::BufWriter::new(io::stdout().lock());
    for entry in entries.iter().filter(|e| e.cmd_for(family).is_some()) {
        print(&mut out, entry, family)?;
    }
    out.flush()?;

    Ok(())
}

fn print(out: &mut impl Write, entry: &Entry, family: ShellFamily) -> io::Result<()> {
    let cmd = entry.cmd_for(family).expect("caller filtered on this");
    let danger = if entry.danger { "  [destructive]" } else { "" };

    writeln!(out, "{}{danger}", entry.id)?;
    writeln!(out, "  {}", entry.desc)?;
    writeln!(out, "  {cmd}")?;

    for name in crate::params::names(cmd) {
        let desc = entry
            .params
            .get(&name)
            .and_then(|spec| spec.desc.as_deref())
            .unwrap_or("no description");
        writeln!(out, "    <{name}>  {desc}")?;
    }

    if !entry.tags.is_empty() {
        writeln!(out, "  tags: {}", entry.tags.join(", "))?;
    }

    writeln!(out)
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
