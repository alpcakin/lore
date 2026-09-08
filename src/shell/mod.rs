//! Shell integration: the snippet each shell needs, and where it goes.
//!
//! The keybinding has to live inside the shell process, so there is no way to
//! set it up without touching a profile file. Every tool in this category works
//! the same way.

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use directories::BaseDirs;

use crate::model::ShellFamily;

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

/// Wraps the generated block so setup can find and remove it later.
const BEGIN: &str = "# >>> lore >>>";
const END: &str = "# <<< lore <<<";

impl From<Shell> for ShellFamily {
    fn from(shell: Shell) -> Self {
        match shell {
            Shell::Bash | Shell::Zsh | Shell::Fish => ShellFamily::Posix,
            Shell::PowerShell => ShellFamily::PowerShell,
        }
    }
}

impl Shell {
    fn label(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
            Shell::PowerShell => "powershell",
        }
    }
}

/// The integration code for a shell.
///
/// Compiled in and returned verbatim. This runs on every shell start, so it
/// reads no files and does no work beyond printing: a slow one is the most
/// common reason people uninstall tools of this kind.
pub fn snippet(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => include_str!("../../assets/shell/bash.sh"),
        Shell::Zsh => include_str!("../../assets/shell/zsh.zsh"),
        Shell::Fish => include_str!("../../assets/shell/fish.fish"),
        Shell::PowerShell => include_str!("../../assets/shell/powershell.ps1"),
    }
}

/// The single line a profile needs.
///
/// The snippet is fetched from the binary rather than written into the profile
/// so that an upgraded binary cannot disagree with a stale copy on disk.
pub fn init_line(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => r#"eval "$(lore init bash)""#,
        Shell::Zsh => r#"eval "$(lore init zsh)""#,
        Shell::Fish => "lore init fish | source",
        Shell::PowerShell => "Invoke-Expression (& lore init powershell | Out-String)",
    }
}

/// The shell that invoked lore, as far as it can be told.
pub fn detect() -> Option<Shell> {
    if let Ok(path) = env::var("SHELL")
        && let Some(shell) = from_program(&path)
    {
        return Some(shell);
    }

    // Git Bash and WSL both set SHELL, so reaching here on Windows means the
    // caller is a PowerShell host.
    cfg!(windows).then_some(Shell::PowerShell)
}

fn from_program(path: &str) -> Option<Shell> {
    let name = Path::new(path).file_stem()?.to_str()?.to_lowercase();
    match name.as_str() {
        "bash" | "sh" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "pwsh" | "powershell" => Some(Shell::PowerShell),
        _ => None,
    }
}

/// A profile file the integration can be written into.
pub struct Profile {
    pub path: PathBuf,
    /// How to name this file when talking to the user.
    pub label: String,
}

/// Every profile that needs the integration for this shell.
///
/// PowerShell returns more than one: Windows PowerShell 5.1 and PowerShell 7
/// keep entirely separate profiles, and a machine commonly has both.
pub fn profiles(shell: Shell) -> Result<Vec<Profile>> {
    let dirs = BaseDirs::new().context("could not determine the home directory")?;
    let home = dirs.home_dir();

    let profile = |path: PathBuf, label: &str| Profile {
        path,
        label: label.to_string(),
    };

    match shell {
        Shell::Bash => Ok(vec![profile(home.join(".bashrc"), shell.label())]),
        Shell::Zsh => {
            let base = env::var_os("ZDOTDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.to_path_buf());
            Ok(vec![profile(base.join(".zshrc"), shell.label())])
        }
        Shell::Fish => {
            let base = env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            Ok(vec![profile(
                base.join("fish").join("config.fish"),
                shell.label(),
            )])
        }
        Shell::PowerShell => powershell_profiles(),
    }
}

/// Asks each installed PowerShell host where its profile lives.
///
/// The path cannot be assembled by hand: `Documents` is frequently redirected
/// into OneDrive, and only the host itself knows where it ended up.
fn powershell_profiles() -> Result<Vec<Profile>> {
    let hosts = [
        ("powershell", "Windows PowerShell 5.1"),
        ("pwsh", "PowerShell 7"),
    ];

    let found: Vec<Profile> = hosts
        .into_iter()
        .filter_map(|(program, label)| {
            ask_profile_path(program).map(|path| Profile {
                path,
                label: label.to_string(),
            })
        })
        .collect();

    if found.is_empty() {
        bail!("no PowerShell host was found on PATH");
    }
    Ok(found)
}

fn ask_profile_path(program: &str) -> Option<PathBuf> {
    let output = Command::new(program)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$PROFILE.CurrentUserCurrentHost",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Adds the integration to every profile for `shell`.
pub fn install(shell: Shell, assume_yes: bool) -> Result<()> {
    for profile in profiles(shell)? {
        install_one(shell, &profile, assume_yes)?;
    }

    if let Some(warning) = execution_policy_warning(shell) {
        println!();
        println!("{warning}");
    }

    Ok(())
}

fn install_one(shell: Shell, profile: &Profile, assume_yes: bool) -> Result<()> {
    let existing = fs::read_to_string(&profile.path).unwrap_or_default();
    if existing.contains(BEGIN) {
        println!(
            "{}: already set up ({})",
            profile.label,
            profile.path.display()
        );
        return Ok(());
    }

    let block = block(shell);
    println!("{}: {}", profile.label, profile.path.display());
    println!("The following will be appended:");
    println!("{block}");

    if !assume_yes && !confirm("Append it?")? {
        println!("{}: skipped", profile.label);
        return Ok(());
    }

    if profile.path.exists() {
        let backup = back_up(&profile.path)?;
        println!("{}: backed up to {}", profile.label, backup.display());
    } else if let Some(parent) = profile.path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile.path)
        .with_context(|| format!("failed to open {}", profile.path.display()))?;
    file.write_all(block.as_bytes())
        .with_context(|| format!("failed to write {}", profile.path.display()))?;

    println!("{}: done, open a new shell and press ctrl+g", profile.label);
    Ok(())
}

/// Removes the integration from every profile for `shell`.
pub fn uninstall(shell: Shell) -> Result<()> {
    for profile in profiles(shell)? {
        let Ok(existing) = fs::read_to_string(&profile.path) else {
            continue;
        };
        if !existing.contains(BEGIN) {
            println!("{}: nothing to remove", profile.label);
            continue;
        }

        let backup = back_up(&profile.path)?;
        fs::write(&profile.path, strip(&existing))
            .with_context(|| format!("failed to write {}", profile.path.display()))?;
        println!(
            "{}: removed ({} kept as {})",
            profile.label,
            profile.path.display(),
            backup.display()
        );
    }

    Ok(())
}

fn block(shell: Shell) -> String {
    format!("\n{BEGIN}\n{}\n{END}\n", init_line(shell))
}

/// Drops the marked block, leaving everything the user wrote untouched.
fn strip(existing: &str) -> String {
    let mut out = String::with_capacity(existing.len());
    let mut inside = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == BEGIN {
            inside = true;
            continue;
        }
        if trimmed == END {
            inside = false;
            continue;
        }
        if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

fn back_up(path: &Path) -> Result<PathBuf> {
    let backup = path.with_extension(format!(
        "{}.lore-backup",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    fs::copy(path, &backup).with_context(|| format!("failed to back up {}", path.display()))?;
    Ok(backup)
}

fn confirm(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// A restricted execution policy stops profile scripts from running at all, so
/// the integration would be installed and silently do nothing.
fn execution_policy_warning(shell: Shell) -> Option<String> {
    if shell != Shell::PowerShell {
        return None;
    }

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-ExecutionPolicy",
        ])
        .output()
        .ok()?;
    let policy = String::from_utf8_lossy(&output.stdout).trim().to_string();

    ["restricted", "allsigned"]
        .contains(&policy.to_lowercase().as_str())
        .then(|| {
            format!(
                "Warning: the PowerShell execution policy is {policy}, so profile scripts never \
                 run.\nAllow them with: Set-ExecutionPolicy -Scope CurrentUser RemoteSigned"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Shell; 4] = [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell];

    #[test]
    fn every_shell_binds_the_chord() {
        for shell in ALL {
            let snippet = snippet(shell);
            assert!(!snippet.is_empty(), "{shell:?} has no snippet");
            assert!(
                snippet.contains("lore pick"),
                "{shell:?} never calls the picker"
            );
        }
    }

    /// A carriage return inside these snippets breaks them at source, which is
    /// why the repository pins line endings.
    #[test]
    fn snippets_never_carry_carriage_returns() {
        for shell in ALL {
            assert!(
                !snippet(shell).contains('\r'),
                "{shell:?} snippet contains a carriage return"
            );
        }
    }

    /// The history is positional, so a blank entry would not be ignored: it
    /// would shift every command after it by one. Windows PowerShell drops an
    /// empty string from a native command's argument list outright.
    #[test]
    fn powershell_drops_blank_history_entries() {
        let snippet = snippet(Shell::PowerShell);
        assert!(
            snippet.contains("Where-Object { $_ }"),
            "the history reaches lore unfiltered"
        );
    }

    /// Windows rebuilds a child's argument list out of a single string, so a
    /// command ending in a backslash escapes the quote meant to close it. The
    /// history travels in a file everywhere rather than only where it has to.
    #[test]
    fn every_snippet_hands_its_history_over_in_a_file() {
        for shell in ALL {
            let snippet = snippet(shell);
            assert!(
                snippet.contains("--history"),
                "{shell:?} does not pass a history file"
            );
            assert!(
                snippet.contains("rm -f") || snippet.contains("Remove-Item"),
                "{shell:?} leaves its history file behind"
            );
        }
    }

    #[test]
    fn every_shell_knows_how_to_load_its_snippet() {
        for shell in ALL {
            assert!(init_line(shell).contains("lore init"));
            assert!(init_line(shell).contains(shell.label()));
        }
    }

    #[test]
    fn stripping_removes_only_the_marked_block() {
        let profile = format!(
            "export EDITOR=vim\n\n{BEGIN}\n{}\n{END}\nalias ll='ls -la'\n",
            init_line(Shell::Bash)
        );

        assert_eq!(strip(&profile), "export EDITOR=vim\n\nalias ll='ls -la'\n");
    }

    #[test]
    fn installing_then_stripping_returns_the_original() {
        let original = "export EDITOR=vim\nalias ll='ls -la'\n";
        let installed = format!("{original}{}", block(Shell::Zsh));

        assert!(installed.contains(BEGIN));
        assert_eq!(strip(&installed).trim_end(), original.trim_end());
    }

    #[test]
    fn stripping_a_profile_without_the_block_changes_nothing() {
        let original = "export EDITOR=vim\n";
        assert_eq!(strip(original), original);
    }

    #[test]
    fn shells_are_recognised_by_their_program_name() {
        assert_eq!(from_program("/bin/bash"), Some(Shell::Bash));
        assert_eq!(from_program("/usr/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(from_program("/usr/local/bin/fish"), Some(Shell::Fish));
        assert_eq!(
            from_program(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            Some(Shell::PowerShell)
        );
        assert_eq!(from_program("/usr/bin/nu"), None);
    }

    #[test]
    fn posix_shells_share_a_dialect_and_powershell_does_not() {
        assert_eq!(ShellFamily::from(Shell::Bash), ShellFamily::Posix);
        assert_eq!(ShellFamily::from(Shell::Zsh), ShellFamily::Posix);
        assert_eq!(ShellFamily::from(Shell::Fish), ShellFamily::Posix);
        assert_eq!(
            ShellFamily::from(Shell::PowerShell),
            ShellFamily::PowerShell
        );
    }
}
