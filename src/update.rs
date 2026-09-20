//! Telling people a newer lore exists, without ever installing one.
//!
//! lore never replaces its own binary. A tool that quietly downloads and runs
//! new code is a way in for anyone who takes over the project's releases, it
//! fights whichever package manager installed it, and a keystroke that has to
//! open a panel is no place to wait on a download. So this only ever prints a
//! line naming the command the user would run themselves.
//!
//! Only a new first or second number is worth saying anything about. Patch
//! releases are frequent and each one would be another line in front of
//! somebody who did not ask.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::store;

/// Set to anything to never look for a newer version.
const NO_CHECK: &str = "LORE_NO_UPDATE_CHECK";

/// Where the last answer is kept, so the picker reads a file rather than the
/// network.
const CACHE: &str = ".lore-latest";

/// How long an answer is trusted before it is worth asking again.
const FRESH: Duration = Duration::from_secs(24 * 60 * 60);

/// The project's own repository, which is where releases are announced.
const REPOSITORY: &str = "https://github.com/alpcakin/lore";

/// A released version, compared by what the release numbers mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl Version {
    /// Parses `1.2.3`, with or without a leading `v`. Anything else, such as
    /// a release candidate, is not a version this offers to anyone.
    pub fn parse(text: &str) -> Option<Self> {
        let mut numbers = text.trim().trim_start_matches('v').split('.');
        let mut next = || numbers.next()?.parse::<u32>().ok();

        let version = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        numbers.next().is_none().then_some(version)
    }

    pub fn this_build() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the crate's own version should parse")
    }

    /// Whether `self` is a release worth interrupting someone about: a new
    /// first or second number, never a third.
    fn worth_announcing_over(&self, current: &Self) -> bool {
        (self.major, self.minor) > (current.major, current.minor)
    }

    fn label(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What `lore version` prints: this version, the newest release, and what to
/// do about the difference.
pub fn status() -> String {
    let current = Version::this_build();
    let exe = env::current_exe().ok();
    let latest = check().ok().flatten();
    report(&current, latest, exe.as_deref())
}

fn report(current: &Version, latest: Option<Version>, exe: Option<&Path>) -> String {
    let mut lines = vec![format!("lore {current}")];

    match latest {
        None => {
            lines.push("Could not check for a newer release".to_string());
            lines.push(format!("Releases are listed at {REPOSITORY}/releases"));
        }
        Some(latest) if latest > *current => {
            lines.push(format!("The newest release is {latest}"));
            lines.push(match exe {
                Some(exe) => upgrade_command(exe),
                None => format!("Upgrade from {REPOSITORY}/releases"),
            });
        }
        Some(_) => lines.push("This is the newest release".to_string()),
    }

    lines.join("\n")
}

/// The line to show, or nothing at all.
pub fn notice() -> Option<String> {
    if turned_off() {
        return None;
    }

    let latest = Version::parse(&fs::read_to_string(cache().ok()?).ok()?)?;
    advice(&Version::this_build(), &latest, &env::current_exe().ok()?)
}

/// What to tell someone running `current` when `latest` is out.
fn advice(current: &Version, latest: &Version, exe: &Path) -> Option<String> {
    if !latest.worth_announcing_over(current) {
        return None;
    }
    Some(format!(
        "lore {} is out. {}",
        latest.label(),
        upgrade_command(exe)
    ))
}

/// How to upgrade an install that lives at `exe`.
///
/// Whoever installed lore through a package manager has to upgrade it through
/// the same one, or the package manager is left believing something that is no
/// longer true.
fn upgrade_command(exe: &Path) -> String {
    let path = exe.to_string_lossy().replace('\\', "/");

    if path.contains("/Cellar/") || path.contains("/homebrew/") || path.contains("/linuxbrew/") {
        "Run: brew upgrade lore".to_string()
    } else if path.contains("/scoop/") {
        "Run: scoop update lore".to_string()
    } else if path.contains("/.cargo/") {
        "Run: cargo install cmdlore --force".to_string()
    } else {
        format!("Upgrade from {REPOSITORY}/releases")
    }
}

fn turned_off() -> bool {
    env::var_os(NO_CHECK).is_some_and(|value| !value.is_empty())
}

fn cache() -> anyhow::Result<PathBuf> {
    Ok(store::sync_dir()?.with_file_name(CACHE))
}

/// Looks for a newer release in the background, at most once a day.
///
/// Through `git ls-remote`, which reads the public tags of the repository
/// without a token and without an API to be rate limited by. Where there is no
/// git there is no check, which is the same as saying nothing.
pub fn refresh_in_background() {
    if turned_off() {
        return;
    }
    let Ok(cache) = cache() else {
        return;
    };

    let fresh = fs::metadata(&cache)
        .and_then(|meta| meta.modified())
        .is_ok_and(|at| at.elapsed().is_ok_and(|age| age < FRESH));
    if fresh {
        return;
    }

    let Ok(exe) = env::current_exe() else {
        return;
    };
    let mut command = Command::new(exe);
    command
        .args(["check-update", "--background"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        command.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }

    let _ = command.spawn();
}

/// Asks the repository for its newest release and records the answer.
pub fn check() -> anyhow::Result<Option<Version>> {
    let cache = cache()?;
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", REPOSITORY, "v*"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }

    let latest = newest(&String::from_utf8_lossy(&output.stdout));
    // Written either way: an answer of "nothing newer" is still an answer, and
    // recording it keeps the check to once a day rather than every time.
    fs::write(&cache, latest.unwrap_or_else(Version::this_build).label())?;
    Ok(latest)
}

/// The highest release in the output of `git ls-remote --tags`.
fn newest(refs: &str) -> Option<Version> {
    refs.lines()
        .filter_map(|line| line.rsplit("refs/tags/").next())
        .filter_map(Version::parse)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("should parse")
    }

    #[test]
    fn versions_are_read_and_ordered_by_what_they_mean() {
        assert_eq!(version("v0.2.2"), version("0.2.2"));
        assert!(version("0.10.0") > version("0.9.9"));
        assert!(version("1.0.0") > version("0.99.99"));
        assert!(Version::parse("0.2").is_none());
        assert!(Version::parse("0.3.0-rc1").is_none());
        assert!(Version::parse("nightly").is_none());
    }

    /// Patch releases go out often. Every one of them saying so in front of
    /// somebody trying to find a command would be noise.
    #[test]
    fn only_a_new_first_or_second_number_is_worth_saying() {
        let current = version("0.2.2");
        let exe = Path::new("/opt/homebrew/Cellar/lore/0.2.2/bin/lore");

        assert!(advice(&current, &version("0.2.3"), exe).is_none());
        assert!(advice(&current, &version("0.2.99"), exe).is_none());
        assert!(advice(&current, &version("0.2.2"), exe).is_none());
        assert!(advice(&current, &version("0.1.9"), exe).is_none());

        assert!(advice(&current, &version("0.3.0"), exe).is_some());
        assert!(advice(&current, &version("1.0.0"), exe).is_some());
    }

    #[test]
    fn the_advice_names_the_version_and_the_right_command() {
        let current = version("0.2.2");
        let said = |exe: &str| advice(&current, &version("0.3.0"), Path::new(exe)).unwrap();

        assert!(said("/opt/homebrew/Cellar/lore/0.2.2/bin/lore").contains("lore 0.3.0 is out"));
        assert!(said("/opt/homebrew/Cellar/lore/0.2.2/bin/lore").contains("brew upgrade lore"));
        assert!(said("/home/alp/.cargo/bin/lore").contains("cargo install cmdlore --force"));
        assert!(
            said(r"C:\Users\alp\scoop\apps\lore\current\lore.exe").contains("scoop update lore")
        );
        assert!(said("/home/alp/.local/bin/lore").contains("releases"));
    }

    #[test]
    fn the_version_command_says_where_you_stand() {
        let current = version("0.2.2");
        let exe = Path::new("/opt/homebrew/Cellar/lore/0.2.2/bin/lore");

        let behind = report(&current, Some(version("0.3.0")), Some(exe));
        assert!(behind.starts_with("lore 0.2.2\n"), "{behind}");
        assert!(behind.contains("newest release is 0.3.0"), "{behind}");
        assert!(behind.contains("brew upgrade lore"), "{behind}");

        // A patch release says nothing in the picker, but somebody who asked
        // outright is told about it.
        let patch = report(&current, Some(version("0.2.3")), Some(exe));
        assert!(patch.contains("newest release is 0.2.3"), "{patch}");
        assert!(patch.contains("brew upgrade lore"), "{patch}");

        let current_release = report(&current, Some(current), Some(exe));
        assert!(
            current_release.contains("This is the newest release"),
            "{current_release}"
        );

        let offline = report(&current, None, Some(exe));
        assert!(offline.contains("Could not check"), "{offline}");
        assert!(offline.contains("/releases"), "{offline}");
    }

    #[test]
    fn the_newest_tag_wins_whatever_order_they_arrive_in() {
        let refs = "a1\trefs/tags/v0.1.0\nb2\trefs/tags/v0.10.1\nc3\trefs/tags/v0.9.0\nd4\trefs/tags/broken\n";
        assert_eq!(newest(refs), Version::parse("0.10.1"));
        assert!(newest("").is_none());
    }
}
