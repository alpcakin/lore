//! Sync between two machines, end to end, against a bare repository on disk
//! standing in for GitHub.
//!
//! Each machine is its own config and data directory. Automatic syncing is
//! off, so every sync in a test is one the test asked for and nothing happens
//! behind its back.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct World {
    root: PathBuf,
    remote: PathBuf,
}

impl World {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("lore-sync-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the world should be creatable");

        let remote = root.join("remote.git");
        let init = Command::new("git")
            .args(["init", "--quiet", "--bare", "--initial-branch=main"])
            .arg(&remote)
            .output()
            .expect("git should run");
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );

        fs::write(root.join("gitconfig"), "").unwrap();
        Self { root, remote }
    }

    fn machine(&self, name: &str) -> Machine<'_> {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).unwrap();
        Machine { world: self, dir }
    }

    fn url(&self) -> String {
        self.remote.display().to_string()
    }

    /// The library as the repository holds it.
    fn published(&self) -> String {
        let output = Command::new("git")
            .arg("--git-dir")
            .arg(&self.remote)
            .args(["show", "main:commands.yaml"])
            .output()
            .expect("git should run");
        assert!(output.status.success(), "nothing was pushed");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Machine<'a> {
    world: &'a World,
    dir: PathBuf,
}

impl Machine<'_> {
    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lore"))
            .args(arguments)
            .env("LORE_CONFIG_DIR", &self.dir)
            .env("LORE_DATA_DIR", &self.dir)
            .env("LORE_NO_AUTO_SYNC", "1")
            // Whoever runs the suite keeps their git settings to themselves.
            .env("GIT_CONFIG_GLOBAL", self.world.root.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(std::process::Stdio::null())
            .output()
            .expect("the binary should run")
    }

    fn ok(&self, arguments: &[&str]) -> String {
        let output = self.run(arguments);
        assert!(
            output.status.success(),
            "{arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn save(&self, command: &str, desc: &str) {
        self.ok(&["save", command, "--desc", desc]);
    }

    fn library(&self) -> String {
        fs::read_to_string(self.dir.join("commands.yaml")).unwrap_or_default()
    }

    fn has(&self, id: &str) -> bool {
        self.library().contains(&format!("id: {id}\n"))
    }

    fn sync_dir(&self) -> PathBuf {
        self.dir.join("sync")
    }
}

fn git(dir: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_first_machine_publishes_its_library() {
    let world = World::new("publish");
    let laptop = world.machine("laptop");
    laptop.save("echo one", "Say one");

    let said = laptop.ok(&["sync", "init", &world.url()]);

    assert!(said.contains("1 change from this one"), "said {said}");
    assert!(world.published().contains("cmd: echo one"));
    assert!(laptop.has("user.echo-one"), "the library itself was lost");
}

#[test]
fn a_second_machine_gets_the_library_and_adds_its_own() {
    let world = World::new("second");
    let laptop = world.machine("laptop");
    let server = world.machine("server");

    laptop.save("echo one", "Say one");
    laptop.ok(&["sync", "init", &world.url()]);

    server.save("echo two", "Say two");
    server.ok(&["sync", "init", &world.url()]);
    assert!(
        server.has("user.echo-one"),
        "the server never got the laptop's command"
    );
    assert!(
        server.has("user.echo-two"),
        "the server lost its own command"
    );

    laptop.ok(&["sync"]);
    assert!(
        laptop.has("user.echo-two"),
        "the laptop never got the server's command"
    );
}

/// Both machines add a command before either syncs, so both append to the end
/// of the same list. Git alone calls that a conflict.
#[test]
fn commands_saved_on_two_machines_at_once_are_both_kept() {
    let world = World::new("concurrent");
    let laptop = world.machine("laptop");
    let server = world.machine("server");
    laptop.ok(&["sync", "init", &world.url()]);
    server.ok(&["sync", "init", &world.url()]);

    laptop.save("echo laptop", "From the laptop");
    server.save("echo server", "From the server");

    laptop.ok(&["sync"]);
    server.ok(&["sync"]);
    laptop.ok(&["sync"]);

    for machine in [&laptop, &server] {
        assert!(machine.has("user.echo-laptop"), "{}", machine.library());
        assert!(machine.has("user.echo-server"), "{}", machine.library());
    }
}

#[test]
fn a_removal_reaches_the_other_machine() {
    let world = World::new("removal");
    let laptop = world.machine("laptop");
    let server = world.machine("server");

    laptop.save("echo one", "Say one");
    laptop.save("echo two", "Say two");
    laptop.ok(&["sync", "init", &world.url()]);
    server.ok(&["sync", "init", &world.url()]);

    laptop.ok(&["rm", "user.echo-one"]);
    laptop.ok(&["sync"]);
    server.ok(&["sync"]);

    assert!(!server.has("user.echo-one"), "{}", server.library());
    assert!(server.has("user.echo-two"));
}

#[test]
fn an_entry_edited_on_both_machines_is_kept_twice_and_settles() {
    let world = World::new("conflict");
    let laptop = world.machine("laptop");
    let server = world.machine("server");

    laptop.save("echo one", "Say one");
    laptop.ok(&["sync", "init", &world.url()]);
    server.ok(&["sync", "init", &world.url()]);

    laptop.ok(&["edit", "user.echo-one", "--desc", "Laptop wording"]);
    server.ok(&["edit", "user.echo-one", "--desc", "Server wording"]);

    laptop.ok(&["sync"]);
    let said = server.ok(&["sync"]);
    assert!(said.contains("user.echo-one-2"), "said {said}");

    laptop.ok(&["sync"]);
    let again = server.ok(&["sync"]);
    assert!(
        again.contains("Already up to date"),
        "never settled: {again}"
    );

    for machine in [&laptop, &server] {
        let library = machine.library();
        assert!(library.contains("Laptop wording"), "{library}");
        assert!(library.contains("Server wording"), "{library}");
        assert_eq!(library.matches("id: user.echo-one").count(), 2, "{library}");
    }
}

#[test]
fn syncing_before_setting_it_up_says_what_to_do() {
    let world = World::new("unset");
    let laptop = world.machine("laptop");

    let output = laptop.run(&["sync"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("lore sync init"));

    let status = laptop.ok(&["sync", "status"]);
    assert!(status.contains("not set up"), "said {status}");
}

#[test]
fn switching_repositories_needs_a_disconnect_first() {
    let world = World::new("switch");
    let laptop = world.machine("laptop");
    laptop.ok(&["sync", "init", &world.url()]);

    let other = world.root.join("other.git");
    let output = laptop.run(&["sync", "init", &other.display().to_string()]);
    assert!(!output.status.success(), "switched without being asked to");
    assert!(String::from_utf8_lossy(&output.stderr).contains("disconnect"));

    laptop.save("echo kept", "Stays");
    laptop.ok(&["sync", "disconnect"]);
    assert!(!laptop.sync_dir().exists());
    assert!(
        laptop.has("user.echo-kept"),
        "disconnecting took the library with it"
    );
}

/// Nobody is watching a background sync, so it must not fail loudly or wait
/// for a password. It leaves the reason where status can report it.
#[test]
fn a_background_sync_that_fails_leaves_a_note() {
    let world = World::new("background");
    let laptop = world.machine("laptop");
    laptop.ok(&["sync", "init", &world.url()]);

    let gone = world.root.join("gone.git");
    git(
        &laptop.sync_dir(),
        &["remote", "set-url", "origin", &gone.display().to_string()],
    );

    let output = laptop.run(&["sync", "--background"]);
    assert!(output.status.success(), "a background sync exited non zero");

    let status = laptop.ok(&["sync", "status"]);
    assert!(status.contains("failed"), "said {status}");

    git(
        &laptop.sync_dir(),
        &["remote", "set-url", "origin", &world.url()],
    );
    laptop.ok(&["sync"]);
    let status = laptop.ok(&["sync", "status"]);
    assert!(
        !status.contains("failed"),
        "the note outlived a good sync: {status}"
    );
}

/// A repository someone created on GitHub with a README already has history,
/// but no library in it yet.
#[test]
fn a_repository_that_already_has_other_files_is_used_as_it_is() {
    let world = World::new("readme");
    let seed = world.root.join("seed");
    let clone = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(&world.remote)
        .arg(&seed)
        .output()
        .unwrap();
    assert!(clone.status.success());
    fs::write(seed.join("README.md"), "my library\n").unwrap();
    git(&seed, &["add", "README.md"]);
    git(
        &seed,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "--quiet",
            "-m",
            "readme",
        ],
    );
    git(
        &seed,
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );

    let laptop = world.machine("laptop");
    laptop.save("echo one", "Say one");
    laptop.ok(&["sync", "init", &world.url()]);

    assert!(world.published().contains("cmd: echo one"));
}

/// A sync that could not push must not count as agreement. Otherwise the next
/// one sees the command saved here missing from the repository and takes that
/// for a removal made elsewhere.
#[test]
fn a_failed_push_loses_nothing_on_the_next_sync() {
    let world = World::new("offline");
    let laptop = world.machine("laptop");
    let server = world.machine("server");
    laptop.ok(&["sync", "init", &world.url()]);
    server.ok(&["sync", "init", &world.url()]);

    laptop.save("echo offline", "Saved while offline");
    let gone = world.root.join("gone.git");
    git(
        &laptop.sync_dir(),
        &["remote", "set-url", "origin", &gone.display().to_string()],
    );
    assert!(
        !laptop.run(&["sync"]).status.success(),
        "synced with no repository"
    );
    git(
        &laptop.sync_dir(),
        &["remote", "set-url", "origin", &world.url()],
    );

    server.save("echo meanwhile", "Saved elsewhere meanwhile");
    server.ok(&["sync"]);

    laptop.ok(&["sync"]);
    assert!(laptop.has("user.echo-offline"), "{}", laptop.library());
    assert!(laptop.has("user.echo-meanwhile"), "{}", laptop.library());
    assert!(world.published().contains("echo offline"));
}
