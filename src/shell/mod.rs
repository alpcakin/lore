//! Shell integration: the snippet each shell needs, and where it goes.
//!
//! The keybinding has to live inside the shell process, so there is no way to
//! set it up without touching a profile file. Every tool in this category works
//! the same way.

pub mod chord;

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use directories::BaseDirs;

use crate::model::ShellFamily;
use crate::shell::chord::Chord;

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

/// Marks the place in every snippet where the chord goes.
const CHORD: &str = "{{chord}}";

/// Wraps the generated block so setup can find and remove it later.
const BEGIN: &str = "# >>> lore >>>";
const END: &str = "# <<< lore <<<";

/// Directories a shell already has on its PATH before any profile runs.
/// Guarding one of these would be noise.
const SYSTEM_BIN: &[&str] = &[
    "/bin",
    "/sbin",
    "/usr/bin",
    "/usr/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
];

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

/// The integration code for a shell, with the chord written into it.
///
/// Compiled in and substituted once. This runs on every shell start, so it
/// reads no files and does no work beyond printing: a slow one is the most
/// common reason people uninstall tools of this kind.
pub fn snippet(shell: Shell, chord: Chord) -> String {
    let template = match shell {
        Shell::Bash => include_str!("../../assets/shell/bash.sh"),
        Shell::Zsh => include_str!("../../assets/shell/zsh.zsh"),
        Shell::Fish => include_str!("../../assets/shell/fish.fish"),
        Shell::PowerShell => include_str!("../../assets/shell/powershell.ps1"),
    };

    template.replace(CHORD, &chord.render(shell))
}

/// The single line a profile needs.
///
/// The snippet is fetched from the binary rather than written into the profile
/// so that an upgraded binary cannot disagree with a stale copy on disk. The
/// chord travels here rather than in a config file because `init` runs on every
/// shell start and is not allowed to read one.
pub fn init_line(shell: Shell, chord: Chord) -> String {
    // A profile that keeps the default reads exactly as it always did.
    let key = if chord.is_default() {
        String::new()
    } else {
        format!(" --key {chord}")
    };

    match shell {
        Shell::Bash => format!(r#"eval "$(lore init bash{key})""#),
        Shell::Zsh => format!(r#"eval "$(lore init zsh{key})""#),
        Shell::Fish => format!("lore init fish{key} | source"),
        Shell::PowerShell => {
            format!("Invoke-Expression (& lore init powershell{key} | Out-String)")
        }
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
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.to_path_buf());
            Ok(vec![profile(base.join(".zshrc"), shell.label())])
        }
        Shell::Fish => {
            let base = env::var_os("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
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
pub fn install(shell: Shell, chord: Chord, assume_yes: bool) -> Result<()> {
    for profile in profiles(shell)? {
        install_one(shell, chord, &profile, assume_yes)?;
    }

    if let Some(warning) = execution_policy_warning(shell) {
        println!();
        println!("{warning}");
    }

    Ok(())
}

fn install_one(shell: Shell, chord: Chord, profile: &Profile, assume_yes: bool) -> Result<()> {
    let existing = fs::read_to_string(&profile.path).unwrap_or_default();
    if existing.contains(BEGIN) {
        println!(
            "{}: already set up ({})",
            profile.label,
            profile.path.display()
        );
        return Ok(());
    }

    let block = block(shell, chord);
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

    println!(
        "{}: done, open a new shell and press {}",
        profile.label,
        chord.spoken()
    );
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

fn block(shell: Shell, chord: Chord) -> String {
    let mut body = String::new();
    if let Some(guard) = path_guard(shell) {
        body.push_str(&guard);
        body.push('\n');
    }
    body.push_str(&init_line(shell, chord));

    format!("\n{BEGIN}\n{body}\n{END}\n")
}

/// The line that puts the binary's own directory on PATH, when it needs one.
///
/// Ubuntu's `~/.profile` sources `~/.bashrc` and only then adds `~/.local/bin`
/// to PATH, so a login shell reaches the block below with lore not yet
/// findable. The eval produces nothing, no key is bound, and nothing says why.
/// Every WSL terminal and every ssh session is a login shell, so this is the
/// ordinary case rather than an exotic one.
fn path_guard(shell: Shell) -> Option<String> {
    let exe = env::current_exe().ok()?;
    let directory = exe.parent()?.to_str()?;

    guard_line(shell, directory)
}

/// Split from `path_guard` so the quoting can be tested without installing
/// anything anywhere.
fn guard_line(shell: Shell, directory: &str) -> Option<String> {
    // Windows composes a process's PATH before it starts, so a profile always
    // runs with the whole of it.
    if shell == Shell::PowerShell || SYSTEM_BIN.contains(&directory) {
        return None;
    }

    Some(match shell {
        Shell::Fish => {
            let quoted = fish_quoted(directory);
            format!("contains {quoted} $PATH; or set -gx PATH {quoted} $PATH")
        }
        _ => {
            let quoted = posix_quoted(directory);
            format!(r#"case ":$PATH:" in *:{quoted}:*) ;; *) PATH={quoted}:"$PATH" ;; esac"#)
        }
    })
}

/// Wraps a path so a shell reads it literally, whatever it contains.
fn posix_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Fish reads a backslash inside single quotes as an escape, which no other
/// posix shell does.
fn fish_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\\', r"\\").replace('\'', r"\'"))
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

/// Copies a profile aside before it is written to.
///
/// The suffix is appended to the whole file name rather than replacing an
/// extension. Every profile worth backing up is a dotfile, and `with_extension`
/// treats `.bashrc` as having none, which produced `.bashrc..lore-backup`.
fn back_up(path: &Path) -> Result<PathBuf> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lore-backup");
    let backup = path.with_file_name(name);

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

    fn snippet(shell: Shell) -> String {
        super::snippet(shell, Chord::default())
    }

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

    /// A snippet still carrying its placeholder would be handed to the shell
    /// verbatim and bind nothing at all.
    #[test]
    fn no_snippet_reaches_the_shell_with_its_placeholder_intact() {
        for shell in ALL {
            for key in ["ctrl-g", "alt-r"] {
                let chord: Chord = key.parse().expect("should parse");
                let snippet = super::snippet(shell, chord);

                assert!(!snippet.contains(CHORD), "{shell:?} kept the placeholder");
                assert!(
                    snippet.contains(&chord.render(shell)),
                    "{shell:?} never binds {key}"
                );
            }
        }
    }

    /// fish binds twice, once per mode, and a substitution that only reached
    /// the first would leave vi mode dead.
    #[test]
    fn fish_binds_the_chord_in_both_of_its_modes() {
        let chord: Chord = "alt-r".parse().expect("should parse");
        let snippet = super::snippet(Shell::Fish, chord);

        assert_eq!(snippet.matches(&chord.render(Shell::Fish)).count(), 2);
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

    /// The panel is drawn over the prompt row and erased on the way out, so the
    /// prompt has to be put back by hand. Redrawing it only when lore failed
    /// left an accepted command rendered on a bare row with no prompt in front
    /// of it.
    #[test]
    fn powershell_redraws_the_prompt_on_every_path() {
        let snippet = snippet(Shell::PowerShell);
        let redraw = snippet
            .find("InvokePrompt")
            .expect("the prompt is never redrawn");
        let insert = snippet.find("Insert(").expect("nothing is ever inserted");

        assert_eq!(
            snippet.matches("InvokePrompt").count(),
            1,
            "the prompt is redrawn on one path only"
        );
        assert!(redraw < insert, "the prompt lands on top of the insertion");
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

    /// The bug this guards against cost a release: the picker draws a panel
    /// under the prompt, which means asking the terminal where the cursor is,
    /// and that question goes out on stdout. A snippet that captured stdout to
    /// read the result swallowed it, no answer ever came back, and the panel
    /// never opened. It only worked on Windows, where the position is read from
    /// the console rather than asked for.
    #[test]
    fn no_snippet_captures_the_pickers_stdout() {
        for shell in ALL {
            let snippet = snippet(shell);
            assert!(
                snippet.contains("--output"),
                "{shell:?} does not ask for the result in a file"
            );

            let invocation = snippet
                .lines()
                .find(|line| line.contains("lore pick"))
                .expect("every snippet runs the picker")
                .to_string();

            assert!(
                !invocation.contains('='),
                "{shell:?} assigns the picker's output: {invocation}"
            );
            assert!(
                !invocation.contains("$("),
                "{shell:?} captures the picker's output: {invocation}"
            );
            assert!(
                !invocation.contains("(lore"),
                "{shell:?} captures the picker's output: {invocation}"
            );
        }
    }

    /// The result file is the caller's to clean up, and it is one more than the
    /// history file, so both have to be named on the way out.
    /// Saving offers what is on the prompt line before anything in the
    /// history, which only works if every shell actually hands the line over.
    #[test]
    fn every_snippet_puts_the_prompt_line_ahead_of_its_history() {
        let buffers = [
            (Shell::Bash, "READLINE_LINE"),
            (Shell::Zsh, "BUFFER"),
            (Shell::Fish, "(commandline)"),
            (Shell::PowerShell, "GetBufferState"),
        ];

        for (shell, buffer) in buffers {
            let snippet = snippet(shell);
            let line = snippet
                .find(buffer)
                .unwrap_or_else(|| panic!("{shell:?} never reads its prompt line"));
            let history = ["fc -lnr", "history --max", "Get-History"]
                .iter()
                .find_map(|call| snippet.find(call))
                .expect("every snippet reads its history");
            let written = ["> \"$recent\"", "> $recent", "WriteAllLines"]
                .iter()
                .find_map(|call| snippet.find(call))
                .expect("every snippet writes the history file");

            assert!(line < written, "{shell:?} reads its prompt line too late");
            if shell != Shell::PowerShell {
                assert!(
                    line < history,
                    "{shell:?} puts its prompt line after its history"
                );
            }
        }
    }

    /// zle runs a widget's commands with stdin on /dev/null. The picker would
    /// fall back to /dev/tty, which macOS refuses to poll, and wait forever
    /// for a cursor position that never arrives.
    #[test]
    fn zsh_hands_the_picker_its_own_terminal_as_stdin() {
        let invocation = snippet(Shell::Zsh)
            .lines()
            .find(|line| line.contains("lore pick"))
            .expect("the snippet runs the picker")
            .to_string();

        assert!(
            invocation.contains(r#"< "$TTY""#),
            "zsh leaves stdin on /dev/null: {invocation}"
        );
    }

    #[test]
    fn every_snippet_removes_both_of_its_temporary_files() {
        for shell in ALL {
            let snippet = snippet(shell);
            let cleanup = snippet
                .lines()
                .find(|line| line.contains("rm -f") || line.contains("Remove-Item"))
                .expect("every snippet cleans up");

            assert!(
                cleanup.contains("recent") && cleanup.contains("out"),
                "{shell:?} leaves a temporary file behind: {cleanup}"
            );
        }
    }

    /// The cursor offset only reaches the prompt if every snippet asks for it
    /// and then puts it somewhere. A snippet that asked and ignored the answer
    /// would insert the offset as part of the command.
    #[test]
    fn every_snippet_asks_for_the_cursor_and_places_it() {
        let placements = [
            (Shell::Bash, "READLINE_POINT"),
            (Shell::Zsh, "CURSOR"),
            (Shell::Fish, "commandline -C"),
            (Shell::PowerShell, "SetCursorPosition"),
        ];

        for (shell, placement) in placements {
            let snippet = snippet(shell);
            assert!(
                snippet.contains("--print-cursor"),
                "{shell:?} never asks for the cursor"
            );
            assert!(
                snippet.contains(placement),
                "{shell:?} never places the cursor"
            );
        }
    }

    #[test]
    fn every_shell_knows_how_to_load_its_snippet() {
        for shell in ALL {
            let line = init_line(shell, Chord::default());
            assert!(line.contains("lore init"));
            assert!(line.contains(shell.label()));
        }
    }

    /// The chord has to survive into the profile, and a profile that kept the
    /// default has to keep the line it was written with.
    #[test]
    fn only_a_changed_chord_reaches_the_init_line() {
        for shell in ALL {
            assert!(!init_line(shell, Chord::default()).contains("--key"));
            assert!(
                init_line(shell, "alt-r".parse().expect("should parse")).contains("--key alt-r"),
                "{shell:?} loses the chord"
            );
        }
    }

    /// The bug this guards against made lore look broken on every WSL terminal
    /// and every ssh session: Ubuntu's ~/.profile sources ~/.bashrc and only
    /// then adds ~/.local/bin to PATH, so the block ran with lore not yet
    /// findable, the eval produced nothing, and no key was bound.
    #[test]
    fn a_binary_outside_the_system_directories_gets_a_path_guard() {
        for shell in [Shell::Bash, Shell::Zsh] {
            let guard = guard_line(shell, "/home/alp/.local/bin").expect("should guard");
            assert!(guard.contains("'/home/alp/.local/bin'"), "{guard}");
            assert!(guard.contains("PATH="), "{guard}");
        }

        let guard = guard_line(Shell::Fish, "/home/alp/.local/bin").expect("should guard");
        assert!(guard.contains("set -gx PATH"), "{guard}");
    }

    /// Windows composes a process's PATH before it starts, and a shell already
    /// has the system directories, so a guard there is noise.
    #[test]
    fn nothing_is_guarded_that_is_already_reachable() {
        assert_eq!(guard_line(Shell::PowerShell, r"C:	ools\lore"), None);

        for directory in SYSTEM_BIN {
            assert_eq!(guard_line(Shell::Bash, directory), None, "{directory}");
        }
    }

    /// A guard that ran twice would put the directory on PATH twice, and one
    /// that mangled a path with a space in it would put the wrong thing there.
    #[test]
    fn the_guard_is_quoted_and_survives_being_run_twice() {
        let guard = guard_line(Shell::Bash, "/home/o'dd dir/bin").expect("should guard");

        let quoted = r"'/home/o'\''dd dir/bin'";
        assert!(guard.contains(quoted), "{guard}");
        assert_eq!(
            guard.matches(quoted).count(),
            2,
            "the test and the assignment should both be quoted: {guard}"
        );
    }

    /// Fish reads a backslash inside single quotes as an escape, which no other
    /// posix shell does.
    #[test]
    fn fish_escapes_what_the_other_shells_do_not() {
        assert_eq!(posix_quoted(r"/a\b"), r"'/a\b'");
        assert_eq!(fish_quoted(r"/a\b"), r"'/a\\b'");
        assert_eq!(posix_quoted("/a'b"), r"'/a'\''b'");
        assert_eq!(fish_quoted("/a'b"), r"'/a\'b'");
    }

    /// The guard lives inside the markers, so removing the block takes it with
    /// it and leaves the profile as it was.
    #[test]
    fn uninstalling_takes_the_guard_with_it() {
        let original = "export EDITOR=vim
";
        let installed = format!("{original}{}", block(Shell::Bash, Chord::default()));

        assert_eq!(strip(&installed).trim_end(), original.trim_end());
    }

    /// Every profile worth backing up is a dotfile, and with_extension treats
    /// one as having no extension, which produced .bashrc..lore-backup.
    #[test]
    fn a_backup_is_named_after_the_whole_file() {
        let scratch = std::env::temp_dir().join(format!("lore-backup-{}", std::process::id()));
        let _ = fs::create_dir_all(&scratch);

        for name in [".bashrc", "profile.ps1"] {
            let path = scratch.join(name);
            fs::write(
                &path,
                "original
",
            )
            .unwrap();

            let backup = back_up(&path).unwrap();
            assert_eq!(
                backup.file_name().unwrap(),
                std::ffi::OsStr::new(&format!("{name}.lore-backup"))
            );
            assert_eq!(
                fs::read_to_string(&backup).unwrap(),
                "original
"
            );
        }

        let _ = fs::remove_dir_all(&scratch);
    }

    #[test]
    fn stripping_removes_only_the_marked_block() {
        let profile = format!(
            "export EDITOR=vim\n\n{BEGIN}\n{}\n{END}\nalias ll='ls -la'\n",
            init_line(Shell::Bash, Chord::default())
        );

        assert_eq!(strip(&profile), "export EDITOR=vim\n\nalias ll='ls -la'\n");
    }

    #[test]
    fn installing_then_stripping_returns_the_original() {
        let original = "export EDITOR=vim\nalias ll='ls -la'\n";
        let installed = format!("{original}{}", block(Shell::Zsh, Chord::default()));

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
        assert_eq!(from_program("pwsh.exe"), Some(Shell::PowerShell));
        assert_eq!(from_program("/usr/bin/nu"), None);
    }

    /// A backslash only separates directories on Windows, which is also the only
    /// place a path shaped like this can reach SHELL.
    #[cfg(windows)]
    #[test]
    fn a_windows_path_is_split_the_windows_way() {
        assert_eq!(
            from_program(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            Some(Shell::PowerShell)
        );
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
