//! Keeping one library across machines, through a git repository the user
//! owns.
//!
//! lore keeps its own clone of the repository in its data directory and never
//! touches git state anywhere else. The library file stays where it always
//! was; a sync reads it, merges it by entry with what the repository holds,
//! writes the result back, and commits and pushes it from the clone.
//!
//! Git is only the transport. It never merges anything itself, because it
//! merges by line and two machines that each saved a command have both
//! appended to the same list, which git reports as a conflict although nothing
//! clashes. See `merge`.
//!
//! lore never sees a password or a token. It runs the user's own `git`, which
//! already knows their SSH key or stored login.

pub mod merge;

use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};

use crate::store;

/// Name of the library inside the repository.
const LIBRARY: &str = "commands.yaml";

/// Branch used when the repository has none yet.
const DEFAULT_BRANCH: &str = "main";

/// Git setting, stored in the clone's own config, naming the branch to sync.
const BRANCH_KEY: &str = "lore.branch";

/// Git setting holding the commit this machine last agreed with the
/// repository on. The merge base, recorded rather than worked out: a fresh
/// clone's checkout matches the repository without this machine ever having
/// seen it, and treating it as agreed would make everything already in the
/// library look deleted here.
const SYNCED_KEY: &str = "lore.synced";

/// Repository `lore sync init` creates when it can do so itself.
const REPOSITORY_NAME: &str = "lore-library";

/// Who a sync commit is by, unless the user gives the clone an identity.
///
/// Deliberately not the user's own: see `has_identity`. The address is a
/// reserved name that can never belong to anyone, so no GitHub account is
/// ever credited with it.
const COMMIT_NAME: &str = "lore";
const COMMIT_EMAIL: &str = "lore@invalid";

/// Held while a sync runs, so two can never interleave their writes.
const LOCK: &str = ".lore-sync.lock";

/// Older than this, a lock was left by a sync that died.
const STALE_LOCK: Duration = Duration::from_secs(120);

/// Touched after every successful sync.
const LAST_SYNC: &str = ".lore-last-sync";

/// What the last background sync failed with, until one succeeds.
const LAST_ERROR: &str = ".lore-sync-error";

/// How long the picker lets pass before it asks for other machines' changes.
///
/// Fetching on every keystroke that opens it would make the picker wait on the
/// network. This fetches in the background at most this often instead.
const REFRESH: Duration = Duration::from_secs(15 * 60);

/// How a sync was started, which decides how it may fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Someone ran `lore sync` and is watching.
    Interactive,
    /// Started by lore itself after a change. Must never ask anything, since
    /// nobody is there to answer, and reports failure by leaving a note.
    Background,
}

/// What a sync did.
#[derive(Debug, Default)]
pub struct Report {
    pub received: usize,
    pub sent: usize,
    pub kept_both: Vec<(String, String)>,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(match (self.received, self.sent) {
            (0, 0) => "Already up to date".to_string(),
            (received, sent) => format!(
                "Synced: {} from other machines, {} from this one",
                count(received, "change"),
                count(sent, "change")
            ),
        });
        for (id, copy) in &self.kept_both {
            lines.push(format!(
                "{id} was changed on two machines. The one synced first kept the id, \
                 this machine's version is now {copy}"
            ));
        }
        lines.join("\n")
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Whether this machine has sync set up.
pub fn is_configured() -> bool {
    store::sync_dir().is_ok_and(|dir| dir.join(".git").is_dir())
}

/// Connects this machine to a repository, creating one first when no address
/// is given and the GitHub CLI can do it.
pub fn init(url: Option<String>) -> Result<Report> {
    require_git()?;
    let dir = store::sync_dir()?;

    if dir.join(".git").is_dir() {
        let current = remote_url(&dir)?;
        match &url {
            Some(url) if url != &current => bail!(
                "this machine already syncs with {current}. \
                 Run `lore sync disconnect` first to switch"
            ),
            _ => {
                println!("Already syncing with {current}");
                return run(Mode::Interactive);
            }
        }
    }

    let addresses = match url {
        Some(url) => vec![url],
        None => addresses_of(&create_repository()?)?,
    };

    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let url = connect(&dir, &addresses)?;

    let branch = git(
        &dir,
        Mode::Interactive,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .ok()
    .and_then(|reference| reference.strip_prefix("origin/").map(str::to_string))
    .unwrap_or_else(|| DEFAULT_BRANCH.to_string());
    git(&dir, Mode::Interactive, &["config", BRANCH_KEY, &branch])?;

    warn_if_public(&url);
    run(Mode::Interactive)
}

/// Merges this machine's library with the repository's and pushes the result.
pub fn run(mode: Mode) -> Result<Report> {
    let dir = store::sync_dir()?;
    if !dir.join(".git").is_dir() {
        bail!("sync is not set up on this machine. Run `lore sync init` first");
    }
    require_git()?;

    // A background sync that finds another running leaves it to do the work.
    // Someone who typed `lore sync` waits for it instead, then syncs again, so
    // what they are told describes the state after both.
    let _lock = match mode {
        Mode::Background => match Lock::take(&dir)? {
            Some(lock) => lock,
            None => return Ok(Report::default()),
        },
        Mode::Interactive => Lock::wait(&dir)?,
    };

    let result = sync_once(&dir, mode).or_else(|error| {
        // The one failure worth trying again: another machine pushed between
        // this one fetching and pushing. The second attempt merges that too.
        if is_rejected_push(&error) {
            sync_once(&dir, mode)
        } else {
            Err(error)
        }
    });

    match &result {
        Ok(_) => {
            let _ = fs::write(dir.join(LAST_SYNC), "");
            let _ = fs::remove_file(dir.join(LAST_ERROR));
        }
        Err(error) if mode == Mode::Background => {
            let _ = fs::write(dir.join(LAST_ERROR), format!("{error:#}"));
        }
        Err(_) => {}
    }

    result
}

fn sync_once(dir: &Path, mode: Mode) -> Result<Report> {
    let branch = git(dir, mode, &["config", "--get", BRANCH_KEY])
        .unwrap_or_else(|_| DEFAULT_BRANCH.to_string());
    let remote = format!("refs/remotes/origin/{branch}");

    git(dir, mode, &["fetch", "--quiet", "origin"])
        .context("could not fetch from the repository")?;

    let has_remote = resolves(dir, mode, &remote);

    let theirs = if has_remote {
        file_at(dir, mode, &remote)?
    } else {
        None
    };

    // The last state this machine and the repository agreed on. Only ever
    // recorded after a sync succeeds, so a push that failed is never taken for
    // agreement either.
    let base = match git(dir, mode, &["config", "--get", SYNCED_KEY]) {
        Ok(commit) => file_at(dir, mode, &commit)?,
        Err(_) => None,
    };

    let library = store::user_library()?;
    let ours = match fs::read_to_string(&library) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", library.display()));
        }
    };

    let merged = merge::merge(base.as_deref(), &ours, theirs.as_deref())?;

    // Nothing here and nothing there: an empty file is not worth a commit.
    if theirs.is_none() && merged.text.trim().is_empty() {
        return Ok(Report::default());
    }

    if merged.text != ours {
        if let Some(parent) = library.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&library, &merged.text)
            .with_context(|| format!("failed to write {}", library.display()))?;
    }

    // The commit is built on top of whatever the repository holds, so history
    // stays a straight line and the push is always a fast forward.
    if has_remote {
        git(dir, mode, &["reset", "--quiet", "--soft", &remote])?;
    }

    fs::write(dir.join(LIBRARY), &merged.text)
        .with_context(|| format!("failed to write {}", dir.join(LIBRARY).display()))?;
    git(dir, mode, &["add", LIBRARY])?;

    let staged = !git_in(Some(dir), mode)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .context("failed to run git")?
        .success();
    if staged {
        let message = format!("Sync from {}", machine_name());
        let mut commit = git_in(Some(dir), mode);
        if !has_identity(dir, mode) {
            commit.args([
                "-c",
                &format!("user.name={COMMIT_NAME}"),
                "-c",
                &format!("user.email={COMMIT_EMAIL}"),
            ]);
        }
        commit.args(["commit", "--quiet", "--no-verify", "-m", &message]);
        checked(commit, "commit")?;
    }

    let ahead = match (has_remote, resolves(dir, mode, "HEAD")) {
        (_, false) => false,
        (false, true) => true,
        (true, true) => {
            git(dir, mode, &["rev-parse", "HEAD"])? != git(dir, mode, &["rev-parse", &remote])?
        }
    };
    if ahead {
        let target = format!("HEAD:refs/heads/{branch}");
        git(dir, mode, &["push", "--quiet", "origin", &target])
            .context("could not push to the repository")?;
    }

    if let Ok(agreed) = git(dir, mode, &["rev-parse", "HEAD"]) {
        git(dir, mode, &["config", SYNCED_KEY, &agreed])?;
    }

    Ok(Report {
        received: merged.received,
        sent: merged.sent,
        kept_both: merged.kept_both,
    })
}

/// Prints where this machine syncs, when it last did, and why it last failed.
pub fn status() -> Result<()> {
    let dir = store::sync_dir()?;
    if !dir.join(".git").is_dir() {
        println!("Sync is not set up on this machine. Run `lore sync init` to start");
        return Ok(());
    }

    println!("Syncing with {}", remote_url(&dir)?);
    match fs::metadata(dir.join(LAST_SYNC)).and_then(|meta| meta.modified()) {
        Ok(at) => println!("Last synced {}", ago(at)),
        Err(_) => println!("Not synced yet"),
    }
    if let Some(error) = last_error() {
        println!("The last automatic sync failed: {error}");
        println!("Run `lore sync` to try again and see the whole message");
    }
    Ok(())
}

/// Stops syncing on this machine. The library stays, and so does the
/// repository.
pub fn disconnect() -> Result<()> {
    let dir = store::sync_dir()?;
    if !dir.join(".git").is_dir() {
        println!("Sync is not set up on this machine");
        return Ok(());
    }

    let url = remote_url(&dir).unwrap_or_default();
    fs::remove_dir_all(&dir).with_context(|| format!("failed to remove {}", dir.display()))?;
    println!("Stopped syncing with {url}. Your library stays where it is");
    Ok(())
}

/// Why the last background sync failed, if it did.
pub fn last_error() -> Option<String> {
    let dir = store::sync_dir().ok()?;
    let text = fs::read_to_string(dir.join(LAST_ERROR)).ok()?;
    let first = text.lines().next()?.trim();
    (!first.is_empty()).then(|| first.to_string())
}

/// Starts a sync in the background, when sync is set up, and returns at once.
///
/// Used after every change and when the picker opens, so nobody waits on the
/// network. Failures are left for `lore sync status` and the picker to report.
pub fn spawn() {
    if !is_configured() || automatic_sync_is_off() {
        return;
    }
    let Ok(exe) = env::current_exe() else {
        return;
    };

    let mut command = Command::new(exe);
    command
        .args(["sync", "--background"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so closing the terminal does not take a sync
        // down halfway, and ctrl+c at the prompt is not delivered to it.
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

/// Starts a background sync if the last one was long enough ago that other
/// machines may have changed something.
pub fn refresh_if_stale() {
    let Ok(dir) = store::sync_dir() else {
        return;
    };
    let fresh = fs::metadata(dir.join(LAST_SYNC))
        .and_then(|meta| meta.modified())
        .is_ok_and(|at| at.elapsed().is_ok_and(|age| age < REFRESH));
    if !fresh {
        spawn();
    }
}

/// Set to anything to sync only when `lore sync` is run by hand.
const NO_AUTO_SYNC: &str = "LORE_NO_AUTO_SYNC";

fn automatic_sync_is_off() -> bool {
    env::var_os(NO_AUTO_SYNC).is_some_and(|value| !value.is_empty())
}

/// Clones the first of `addresses` that git can log in to.
///
/// A repository has an ssh address and an https one, and which of them works
/// depends on what the user has already set up for git rather than on what
/// they told the GitHub CLI they prefer. Each is tried with prompting off, so
/// one that would sit waiting for a username fails and lets the other be
/// tried. If neither works and the GitHub CLI is there, it is asked to set up
/// git's credentials, which is the missing piece when only https is on offer.
fn connect(dir: &Path, addresses: &[String]) -> Result<String> {
    let mut failures = Vec::new();

    for (attempt, url) in addresses.iter().enumerate() {
        println!("Connecting to {url}");
        match clone(dir, url) {
            Ok(()) => return Ok(url.clone()),
            Err(error) => {
                println!("  that address did not work");
                failures.push(format!("{url}: {error}"));
            }
        }

        // Worth one attempt at teaching git the login the GitHub CLI holds,
        // then trying the addresses again.
        if attempt + 1 == addresses.len() && setup_git_credentials() {
            for url in addresses {
                println!("Connecting to {url}");
                if clone(dir, url).is_ok() {
                    return Ok(url.clone());
                }
            }
        }
    }

    bail!(
        "could not reach the repository.\n  {}\n\n\
         If you use ssh with GitHub, check that `ssh -T git@github.com` greets you. \
         For https, `gh auth login` or a credential helper has to be set up first.",
        failures.join("\n  ")
    )
}

fn clone(dir: &Path, url: &str) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("failed to clear {}", dir.display()))?;
    }

    // Prompting stays off even here: a clone that stops to ask for a username
    // cannot be given up on in favour of an address that needs no password.
    let output = git_in(None, Mode::Background)
        .arg("clone")
        .arg("--quiet")
        .arg(url)
        .arg(dir)
        .output()
        .context("failed to run git")?;

    if !output.status.success() {
        let _ = fs::remove_dir_all(dir);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("git clone failed");
        bail!("{}", reason.trim().trim_start_matches("fatal: "));
    }
    Ok(())
}

/// Asks the GitHub CLI to give git the login it already holds, reporting
/// whether it could.
fn setup_git_credentials() -> bool {
    let done = Command::new("gh")
        .args(["auth", "setup-git"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if done {
        println!("Set up git to use your GitHub CLI login");
    }
    done
}

/// Both addresses of a GitHub repository, the one the user prefers first.
fn addresses_of(name: &str) -> Result<Vec<String>> {
    let protocol = gh_output(&["config", "get", "git_protocol"]).unwrap_or_default();

    let https = gh_output(&["repo", "view", name, "--json", "url", "--jq", ".url"]);
    let ssh = gh_output(&["repo", "view", name, "--json", "sshUrl", "--jq", ".sshUrl"]);

    let mut addresses: Vec<String> = if protocol == "ssh" {
        vec![ssh, https]
    } else {
        vec![https, ssh]
    }
    .into_iter()
    .flatten()
    .collect();
    addresses.dedup();

    if addresses.is_empty() {
        bail!("could not find the address of {name}");
    }
    Ok(addresses)
}

fn gh_output(arguments: &[&str]) -> Option<String> {
    let output = Command::new("gh").args(arguments).output().ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}

/// Makes a private repository with the GitHub CLI, or finds the one an earlier
/// machine made, and returns its address.
fn create_repository() -> Result<String> {
    let instructions = format!(
        "Create an empty private repository, for example {REPOSITORY_NAME} on GitHub, \
         then run:\n\n    lore sync init <its address>\n\n\
         With the GitHub CLI installed and logged in, `lore sync init` makes it for you"
    );

    let signed_in = Command::new("gh")
        .args(["auth", "status"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !signed_in {
        bail!("{instructions}");
    }

    let exists = Command::new("gh")
        .args(["repo", "view", REPOSITORY_NAME, "--json", "name"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());

    if exists {
        println!("Using your existing private repository {REPOSITORY_NAME}");
    } else {
        if !confirm(&format!(
            "Create a private GitHub repository named {REPOSITORY_NAME} for your library?"
        ))? {
            bail!("{instructions}");
        }
        let created = Command::new("gh")
            .args([
                "repo",
                "create",
                REPOSITORY_NAME,
                "--private",
                "--description",
                "My lore command library",
            ])
            .stdout(Stdio::null())
            .output()
            .context("failed to run gh")?;
        if !created.status.success() {
            bail!(
                "could not create the repository: {}",
                String::from_utf8_lossy(&created.stderr).trim()
            );
        }
        println!("Created the private repository {REPOSITORY_NAME}");
    }

    Ok(REPOSITORY_NAME.to_string())
}

/// Saved commands often carry host names, user names and internal addresses,
/// so a public repository is worth a warning. Only GitHub can be asked.
fn warn_if_public(url: &str) {
    let public = Command::new("gh")
        .args([
            "repo",
            "view",
            url,
            "--json",
            "isPrivate",
            "--jq",
            ".isPrivate",
        ])
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "false"
        });
    if public {
        println!(
            "Warning: this repository is public. Saved commands often contain server \
             names and addresses, so consider making it private"
        );
    }
}

fn confirm(question: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(false);
    }
    print!("{question} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn require_git() -> Result<()> {
    let found = Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !found {
        bail!("sync needs git, and git was not found on PATH");
    }
    Ok(())
}

/// A git command with the settings every sync needs, run in `dir`.
///
/// Line endings are left alone so a Windows checkout cannot rewrite the file
/// under the merge. Signing and hooks are skipped because a sync that stops
/// to ask for a passphrase or runs someone's pre-commit checks has stopped
/// being a sync. In the background nothing may prompt: git gives up rather
/// than asking for a password, and ssh rather than asking about a host key.
fn git_in(dir: Option<&Path>, mode: Mode) -> Command {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    command.args(["-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"]);
    command.env("GIT_TERMINAL_PROMPT", "0");

    if mode == Mode::Background {
        command.stdin(Stdio::null());
        let custom_ssh = env::var_os("GIT_SSH_COMMAND").is_some()
            || dir.is_some_and(|dir| {
                Command::new("git")
                    .arg("-C")
                    .arg(dir)
                    .args(["config", "--get", "core.sshCommand"])
                    .output()
                    .is_ok_and(|out| out.status.success())
            });
        if !custom_ssh {
            command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
    }
    command
}

/// Runs git in `dir` and returns its trimmed output.
fn git(dir: &Path, mode: Mode, args: &[&str]) -> Result<String> {
    let mut command = git_in(Some(dir), mode);
    command.args(args);
    checked(command, args.first().copied().unwrap_or("git"))
}

fn checked(mut command: Command, what: &str) -> Result<String> {
    let output = command.output().context("failed to run git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        bail!(GitError {
            what: what.to_string(),
            message: if message.is_empty() {
                format!("git {what} failed")
            } else {
                message.to_string()
            },
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A git command that failed, carrying what it said.
#[derive(Debug)]
struct GitError {
    what: String,
    message: String,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GitError {}

fn is_rejected_push(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<GitError>().is_some_and(|git| {
            git.what == "push"
                && (git.message.contains("rejected")
                    || git.message.contains("fetch first")
                    || git.message.contains("non-fast-forward"))
        })
    })
}

fn resolves(dir: &Path, mode: Mode, reference: &str) -> bool {
    git(dir, mode, &["rev-parse", "--verify", "--quiet", reference]).is_ok()
}

/// The library as it was at `revision`, or `None` if it did not exist there.
fn file_at(dir: &Path, mode: Mode, revision: &str) -> Result<Option<String>> {
    let path = format!("{revision}:{LIBRARY}");
    if !resolves(dir, mode, &path) {
        return Ok(None);
    }
    let output = git_in(Some(dir), mode)
        .args(["show", &path])
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!("could not read {LIBRARY} at {revision}");
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

fn remote_url(dir: &Path) -> Result<String> {
    git(dir, Mode::Interactive, &["remote", "get-url", "origin"])
}

/// Whether the user has given this clone an identity of its own.
///
/// Only the clone's own config counts, never the identity git uses everywhere
/// else. A sync commit is bookkeeping, and one per saved command, so putting
/// the user's name and email on it would file every save as a contribution on
/// their GitHub profile and fill the graph with squares that stand for
/// nothing. Someone who wants their own name on them can say so, by setting
/// `user.name` and `user.email` with `git config` inside the sync directory.
fn has_identity(dir: &Path, mode: Mode) -> bool {
    let local = |key: &str| {
        git(dir, mode, &["config", "--local", "--get", key]).is_ok_and(|value| !value.is_empty())
    };
    local("user.email") && local("user.name")
}

/// A name for this machine in commit messages, so the repository's history
/// says where each change came from.
fn machine_name() -> String {
    env::var("COMPUTERNAME")
        .ok()
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "a machine".to_string())
}

fn ago(at: SystemTime) -> String {
    let seconds = at.elapsed().map(|age| age.as_secs()).unwrap_or(0);
    match seconds {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} ago", count((seconds / 60) as usize, "minute")),
        3600..86400 => format!("{} ago", count((seconds / 3600) as usize, "hour")),
        _ => format!("{} ago", count((seconds / 86400) as usize, "day")),
    }
}

/// A lock file, removed when dropped.
struct Lock(PathBuf);

impl Lock {
    /// Takes the lock, or `None` while another sync holds it.
    fn take(dir: &Path) -> Result<Option<Self>> {
        let path = dir.join(LOCK);
        for _ in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Some(Self(path))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let stale = fs::metadata(&path)
                        .and_then(|meta| meta.modified())
                        .is_ok_and(|at| at.elapsed().is_ok_and(|age| age > STALE_LOCK));
                    if !stale {
                        return Ok(None);
                    }
                    let _ = fs::remove_file(&path);
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to lock {}", path.display()));
                }
            }
        }
        Ok(None)
    }
}

impl Lock {
    /// Takes the lock, waiting for a sync already holding it to finish.
    fn wait(dir: &Path) -> Result<Self> {
        let started = SystemTime::now();
        loop {
            if let Some(lock) = Self::take(dir)? {
                return Ok(lock);
            }
            if started.elapsed().is_ok_and(|waited| waited > STALE_LOCK) {
                bail!("another sync has been running for two minutes, try again later");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_says_what_moved() {
        assert_eq!(Report::default().summary(), "Already up to date");

        let report = Report {
            received: 1,
            sent: 2,
            kept_both: vec![("a".to_string(), "a-2".to_string())],
        };
        let summary = report.summary();
        assert!(
            summary.contains("1 change from other machines, 2 changes from this one"),
            "{summary}"
        );
        assert!(summary.contains("now a-2"), "{summary}");
    }

    #[test]
    fn a_rejected_push_is_recognised_and_nothing_else_is() {
        let rejected = anyhow::Error::new(GitError {
            what: "push".to_string(),
            message: "! [rejected] HEAD -> main (fetch first)".to_string(),
        })
        .context("could not push to the repository");
        assert!(is_rejected_push(&rejected));

        let unreachable = anyhow::Error::new(GitError {
            what: "fetch".to_string(),
            message: "Could not resolve host".to_string(),
        });
        assert!(!is_rejected_push(&unreachable));
    }

    #[test]
    fn a_lock_is_exclusive_until_it_is_dropped() {
        let dir = env::temp_dir().join(format!("lore-lock-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        let first = Lock::take(&dir).unwrap();
        assert!(first.is_some());
        assert!(Lock::take(&dir).unwrap().is_none());
        drop(first);
        assert!(Lock::take(&dir).unwrap().is_some());

        let _ = fs::remove_dir_all(&dir);
    }
}
