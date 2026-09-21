//! End to end tests against the built binary.
//!
//! The unit tests reach every module directly; these check the one thing they
//! cannot, which is that the process a shell actually invokes behaves the way
//! the modules promise.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A config and data directory of this test's own, so nothing touches the
/// library of whoever is running the suite.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("lore-it-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox should be creatable");
        Self { root }
    }

    fn library(&self) -> PathBuf {
        self.root.join("commands.yaml")
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lore"))
            .args(arguments)
            .env("LORE_CONFIG_DIR", &self.root)
            .env("LORE_DATA_DIR", &self.root)
            .output()
            .expect("the binary should run")
    }

    fn succeeds(&self, arguments: &[&str]) -> String {
        let output = self.run(arguments);
        assert!(
            output.status.success(),
            "{arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lore"))
        .args(arguments)
        .output()
        .expect("the binary should run")
}

#[test]
fn help_and_version_are_output_rather_than_failures() {
    for arguments in [["--help"], ["--version"]] {
        let output = run(&arguments);
        assert!(output.status.success(), "{arguments:?} exited non zero");
        assert!(!output.stdout.is_empty(), "{arguments:?} printed nothing");
    }
}

/// The snippet is what a shell evaluates on every start. A leftover placeholder
/// would be sourced verbatim and bind nothing at all.
#[test]
fn every_shell_gets_a_snippet_with_its_own_chord_written_in() {
    let bindings = [
        ("bash", r"\C-g", r"\M-r"),
        ("zsh", "^G", "^[r"),
        ("fish", r"\cg", r"\er"),
        ("powershell", "Ctrl+g", "Alt+r"),
    ];

    for (shell, default, alternative) in bindings {
        let output = run(&["init", shell]);
        let snippet = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "init {shell} failed");
        assert!(!snippet.contains("{{"), "init {shell} kept a placeholder");
        assert!(snippet.contains(default), "init {shell} lost the chord");

        let output = run(&["init", shell, "--key", "alt-r"]);
        let snippet = String::from_utf8_lossy(&output.stdout);
        assert!(
            snippet.contains(alternative),
            "init {shell} --key alt-r bound {snippet}"
        );
    }
}

#[test]
fn a_key_the_terminal_owns_is_refused_before_anything_is_written() {
    let output = run(&["init", "bash", "--key", "ctrl-m"]);
    assert!(!output.status.success(), "ctrl-m was accepted");
}

/// The full life of an entry through the command line, and the reason the file
/// is spliced rather than reserialised: what the user wrote around their
/// entries has to survive every one of these steps.
#[test]
fn an_entry_can_be_saved_edited_and_removed_without_disturbing_the_file() {
    let sandbox = Sandbox::new("lifecycle");

    sandbox.succeeds(&[
        "save",
        "kics scan -p .",
        "--desc",
        "Scan this project",
        "--tags",
        "security, iac",
    ]);

    let library = sandbox.library();
    prepend_comment(&library, "# commands I keep forgetting\n");

    let listed = sandbox.succeeds(&["list", "--shell", "bash"]);
    assert!(listed.contains("Scan this project"), "listed {listed}");
    assert!(listed.contains("security, iac"), "listed {listed}");

    let id = saved_id(&library);
    sandbox.succeeds(&["edit", &id, "--cmd", "kics scan -p . --report-formats json"]);

    let text = fs::read_to_string(&library).expect("the library should still be there");
    assert!(
        text.contains("# commands I keep forgetting"),
        "wrote {text}"
    );
    assert!(text.contains("--report-formats json"), "wrote {text}");
    assert_eq!(
        text.matches(&format!("id: {id}")).count(),
        1,
        "wrote {text}"
    );

    let listed = sandbox.succeeds(&["list", "--shell", "bash"]);
    assert!(listed.contains("Scan this project"), "listed {listed}");

    sandbox.succeeds(&["rm", &id]);
    let text = fs::read_to_string(&library).expect("the library should still be there");
    assert!(
        text.contains("# commands I keep forgetting"),
        "wrote {text}"
    );
    assert!(!text.contains(&format!("id: {id}")), "wrote {text}");
}

/// A builtin cannot be changed inside the binary, so an edit has to land in the
/// user's own library and win from there.
#[test]
fn editing_a_builtin_writes_an_override_into_the_users_library() {
    let sandbox = Sandbox::new("override");
    let listed = sandbox.succeeds(&["list", "--shell", "bash"]);
    let id = listed
        .lines()
        .find(|line| !line.starts_with(' ') && !line.trim().is_empty())
        .expect("the builtin library should not be empty")
        .trim()
        .to_string();

    sandbox.succeeds(&["edit", &id, "--desc", "Mine now"]);

    let text = fs::read_to_string(sandbox.library()).expect("the library should exist");
    assert!(text.contains(&format!("id: {id}")), "wrote {text}");
    assert!(text.contains("Mine now"), "wrote {text}");

    let listed = sandbox.succeeds(&["list", "--shell", "bash"]);
    assert!(listed.contains("Mine now"), "listed {listed}");
}

#[test]
fn an_edit_that_changes_nothing_is_refused() {
    let sandbox = Sandbox::new("no-change");
    let output = sandbox.run(&["edit", "git.log.graph"]);
    assert!(!output.status.success(), "an empty edit was accepted");
}

#[test]
fn an_unknown_id_is_refused_rather_than_created() {
    let sandbox = Sandbox::new("unknown");
    let output = sandbox.run(&["edit", "nothing.here", "--desc", "x"]);
    assert!(!output.status.success(), "an unknown id was edited");
    assert!(!sandbox.library().exists(), "an unknown id wrote a file");
}

fn prepend_comment(library: &Path, comment: &str) {
    let text = fs::read_to_string(library).expect("the library should exist");
    fs::write(library, format!("{comment}{text}")).expect("the library should be writable");
}

fn saved_id(library: &Path) -> String {
    fs::read_to_string(library)
        .expect("the library should exist")
        .lines()
        .find_map(|line| line.trim().strip_prefix("- id: ").map(str::to_string))
        .expect("the saved entry should have an id")
}

/// `lore list | head` closes the pipe while lore is still writing. That is how
/// a pipeline says it has had enough, and it must not surface as a panic.
#[test]
fn a_reader_that_stops_early_is_not_a_crash() {
    use std::process::Stdio;

    let sandbox = Sandbox::new("broken-pipe");
    let mut child = Command::new(env!("CARGO_BIN_EXE_lore"))
        .args(["list", "--shell", "bash"])
        .env("LORE_CONFIG_DIR", &sandbox.root)
        .env("LORE_DATA_DIR", &sandbox.root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should run");

    // Closed before lore has written anything, so its first write fails.
    drop(child.stdout.take());

    let output = child.wait_with_output().expect("the binary should finish");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked"), "lore panicked: {stderr}");
    assert!(output.status.success(), "lore failed: {stderr}");
}

#[test]
fn saving_takes_tags_from_the_command_and_the_description() {
    let sandbox = Sandbox::new("save-tags");
    sandbox.succeeds(&[
        "save",
        "kubectl logs -f api-0",
        "--desc",
        "Follow the api #k8s",
        "--tags",
        "prod",
    ]);

    let text = fs::read_to_string(sandbox.library()).expect("the library should exist");
    assert!(text.contains("desc: Follow the api\n"), "wrote {text}");
    for tag in ["k8s", "prod", "kubectl", "logs"] {
        assert!(text.contains(&format!("- {tag}\n")), "no {tag} in {text}");
    }
}

/// Without --desc the question is asked, but only of a person. A script that
/// left it out has nobody to answer and must not hang waiting.
#[test]
fn saving_without_a_description_from_a_script_fails_instead_of_waiting() {
    let sandbox = Sandbox::new("save-no-desc");
    let output = Command::new(env!("CARGO_BIN_EXE_lore"))
        .args(["save", "git status"])
        .env("LORE_CONFIG_DIR", &sandbox.root)
        .env("LORE_DATA_DIR", &sandbox.root)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the binary should run");

    assert!(!output.status.success(), "saved with no description");
    assert!(!sandbox.library().exists(), "wrote an unfindable entry");
}

#[test]
fn finding_prints_the_matches_best_first() {
    let sandbox = Sandbox::new("find");
    let found = sandbox.succeeds(&["find", "--shell", "bash", "docker", "logs"]);

    assert!(found.contains("docker.logs"), "found {found}");
    assert!(
        found.contains("Watch what a container is printing"),
        "the description is missing: {found}"
    );
    assert!(
        !found.contains("git."),
        "an entry matching neither word was listed: {found}"
    );
}

/// The form a pipeline uses: one command, nothing around it.
#[test]
fn finding_the_first_match_prints_the_command_alone() {
    let sandbox = Sandbox::new("find-first");
    let found = sandbox.succeeds(&["find", "-1", "--shell", "bash", "docker", "logs"]);

    assert_eq!(found.lines().count(), 1, "found {found}");
    assert!(found.starts_with("docker logs"), "found {found}");
}

/// Half a remembered command is a search, not a mistake, and a mistyped
/// command searches rather than being refused outright.
#[test]
fn words_that_are_not_a_command_are_searched_for() {
    let sandbox = Sandbox::new("bare-words");
    let found = sandbox.succeeds(&["kubectl", "logs"]);
    assert!(found.contains("k8s.logs.follow"), "found {found}");

    let nothing = sandbox.run(&["dcoker"]);
    assert!(
        !nothing.status.success(),
        "a search with no match succeeded"
    );
    assert!(!sandbox.library().exists(), "a search wrote something");
}

/// The commands lore has always had must not become search terms.
#[test]
fn a_real_command_is_never_taken_for_a_search() {
    let sandbox = Sandbox::new("not-a-search");

    let listed = sandbox.succeeds(&["list", "--shell", "bash"]);
    assert!(listed.contains("archive.tar.create"), "listed {listed}");

    let version = sandbox.succeeds(&["version"]);
    assert!(version.starts_with("lore "), "said {version}");
}
