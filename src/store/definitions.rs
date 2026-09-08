//! Reading command definition files and merging them into one library.
//!
//! Layers are merged weakest first, so a later layer shadows an earlier one by
//! entry id. Any layer may also hide entries by glob pattern without redefining
//! them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::ops::Range;
use std::path::Path;

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;

use crate::model::{Entry, Layer, Library};

/// Schema version this build understands.
const SCHEMA_VERSION: u32 = 1;

/// Top level key holding the ids a layer hides.
const DISABLED: &str = "disabled:";

/// Libraries compiled into the binary, so a fresh install opens onto a full
/// picker without a network round trip.
const BUILTINS: &[(&str, &str)] = &[(
    "builtin:sample.yaml",
    include_str!("../../assets/builtins/sample.yaml"),
)];

/// Loads the builtin library, overlaying the user's own file when it exists.
pub fn load(user_library: Option<&Path>) -> Result<Vec<Entry>> {
    let mut layers = Vec::new();

    for (origin, source) in BUILTINS {
        layers.push(read(source, origin, Layer::Builtin)?);
    }

    if let Some(path) = user_library
        && path.exists()
    {
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        layers.push(read(&source, &path.display().to_string(), Layer::User)?);
    }

    merge(layers)
}

/// A command on its way into the user's library.
#[derive(Debug, Serialize)]
pub struct NewEntry {
    pub id: String,
    pub cmd: String,
    pub desc: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Appends an entry to the user's library, creating the file if needed.
///
/// The entry is appended as text rather than re-serialising the whole document,
/// because users are told to hand edit and version this file. Round tripping it
/// through a parser would silently delete their comments and reflow everything
/// they had arranged.
pub fn append(path: &Path, entry: &NewEntry) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut out = String::new();

    if existing.trim().is_empty() {
        out.push_str(&format!("version: {SCHEMA_VERSION}\ncommands:\n"));
    } else {
        if !existing.ends_with('\n') {
            out.push('\n');
        }
        if !existing.lines().any(|line| line.trim_end() == "commands:") {
            out.push_str("commands:\n");
        }
    }

    out.push_str(&as_list_item(entry)?);

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(out.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(())
}

/// Renders one entry as an indented YAML list item.
///
/// The body is produced by the serialiser so that quoting and escaping are
/// correct, then indented into place.
fn as_list_item(entry: &NewEntry) -> Result<String> {
    let body = serde_yaml_ng::to_string(entry).context("failed to serialise the entry")?;

    let mut out = String::new();
    for (index, line) in body.lines().enumerate() {
        let prefix = if index == 0 { "  - " } else { "    " };
        out.push_str(prefix);
        out.push_str(line);
        out.push('\n');
    }

    Ok(out)
}

/// Cuts an entry out of a library file, reporting whether it was there.
///
/// Only the lines of that one list item are removed. Reserialising the document
/// would be simpler, but users are told to hand edit and version this file, and
/// a round trip through the parser silently deletes their comments.
pub fn remove(path: &Path, id: &str) -> Result<bool> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let Some(block) = item_of(&text, id) else {
        return Ok(false);
    };

    let kept: Vec<&str> = text
        .lines()
        .enumerate()
        .filter(|(number, _)| !block.contains(number))
        .map(|(_, line)| line)
        .collect();

    let mut out = kept.join("\n");
    out.push('\n');
    fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))?;

    Ok(true)
}

/// Hides an entry the user cannot delete, such as one compiled into the binary.
pub fn disable(path: &Path, id: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let text = fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();

    match lines.iter().position(|line| line.trim_end() == DISABLED) {
        Some(at) => lines.insert(at + 1, format!("  - {id}")),
        None => {
            if text.trim().is_empty() {
                lines.push(format!("version: {SCHEMA_VERSION}"));
            }
            lines.push(DISABLED.to_string());
            lines.push(format!("  - {id}"));
        }
    }

    let mut out = lines.join("\n");
    out.push('\n');
    fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))?;

    Ok(())
}

/// The lines occupied by the list item that declares `id`.
fn item_of(text: &str, id: &str) -> Option<Range<usize>> {
    let lines: Vec<&str> = text.lines().collect();

    for (number, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("- ") {
            continue;
        }

        let indent = indent_of(line);
        // The item runs until something at the same level or shallower, which
        // covers both the next entry and the key that follows the list.
        let end = lines
            .iter()
            .enumerate()
            .skip(number + 1)
            .find(|(_, line)| !line.trim().is_empty() && indent_of(line) <= indent)
            .map_or(lines.len(), |(at, _)| at);

        if declares(&lines[number..end], id) {
            return Some(number..end);
        }
    }

    None
}

fn declares(block: &[&str], id: &str) -> bool {
    block.iter().any(|line| {
        line.trim_start()
            .trim_start_matches("- ")
            .strip_prefix("id:")
            .is_some_and(|value| unquote(value.trim()) == id)
    })
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Turns a command into an id that is stable, readable and not already taken.
pub fn suggest_id(cmd: &str, taken: &BTreeSet<String>) -> String {
    let slug: Vec<String> = cmd
        .split_whitespace()
        .filter(|word| !word.starts_with('-'))
        .take(2)
        .map(|word| {
            word.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();

    let stem = if slug.is_empty() {
        "user.command".to_string()
    } else {
        format!("user.{}", slug.join("-"))
    };

    if !taken.contains(&stem) {
        return stem;
    }
    (2..)
        .map(|n| format!("{stem}-{n}"))
        .find(|candidate| !taken.contains(candidate))
        .expect("the sequence is unbounded")
}

/// Parses one library file and stamps its entries with the layer they came from.
fn read(source: &str, origin: &str, layer: Layer) -> Result<Library> {
    let mut library: Library = serde_yaml_ng::from_str(source)
        .with_context(|| format!("{origin} is not a valid command library"))?;

    if library.version != SCHEMA_VERSION {
        bail!(
            "{origin} declares schema version {}, this build understands {SCHEMA_VERSION}",
            library.version
        );
    }

    let mut seen = BTreeSet::new();
    for entry in &mut library.commands {
        if entry.id.trim().is_empty() {
            bail!("{origin} contains an entry with an empty id");
        }
        if entry.desc.trim().is_empty() {
            bail!("{origin} entry `{}` has an empty description", entry.id);
        }
        if !seen.insert(entry.id.clone()) {
            bail!("{origin} defines `{}` more than once", entry.id);
        }
        entry.layer = layer;
    }

    Ok(library)
}

/// Collapses layers into one library, ordered by id for a stable listing.
fn merge(layers: Vec<Library>) -> Result<Vec<Entry>> {
    let mut by_id: BTreeMap<String, Entry> = BTreeMap::new();
    let mut patterns: Vec<String> = Vec::new();

    for layer in layers {
        patterns.extend(layer.disabled);
        for entry in layer.commands {
            by_id.insert(entry.id.clone(), entry);
        }
    }

    let disabled = globs(&patterns)?;
    Ok(by_id
        .into_values()
        .filter(|entry| !disabled.is_match(&entry.id))
        .collect())
}

fn globs(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .with_context(|| format!("`{pattern}` is not a valid disable pattern"))?;
        builder.add(glob);
    }
    builder
        .build()
        .context("failed to compile disable patterns")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ShellFamily;

    fn read_test(source: &str, layer: Layer) -> Result<Library> {
        read(source, "test.yaml", layer)
    }

    fn ids(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn builtin_library_is_valid() {
        let entries = load(None).expect("builtin library must parse");
        assert_eq!(entries.len(), 20);
        assert!(entries.iter().all(|e| e.layer == Layer::Builtin));
    }

    #[test]
    fn builtin_library_carries_working_placeholders() {
        let entries = load(None).unwrap();
        let logs = entries
            .iter()
            .find(|e| e.id == "docker.logs.follow")
            .expect("docker.logs.follow must exist");

        let cmd = logs.cmd_for(ShellFamily::Posix).unwrap();
        assert_eq!(crate::params::names(cmd), ["lines", "container"]);
    }

    #[test]
    fn builtin_shell_variants_resolve_per_family() {
        let entries = load(None).unwrap();
        let ports = entries
            .iter()
            .find(|e| e.id == "sys.ports.listening")
            .unwrap();

        assert!(
            ports
                .cmd_for(ShellFamily::Posix)
                .unwrap()
                .starts_with("ss ")
        );
        assert!(
            ports
                .cmd_for(ShellFamily::PowerShell)
                .unwrap()
                .starts_with("Get-NetTCPConnection")
        );
    }

    #[test]
    fn a_later_layer_shadows_an_earlier_one() {
        let builtin = read_test(
            "version: 1
commands:
  - id: git.log
    cmd: git log
    desc: builtin version
",
            Layer::Builtin,
        )
        .unwrap();

        let user = read_test(
            "version: 1
commands:
  - id: git.log
    cmd: git log --oneline
    desc: user version
",
            Layer::User,
        )
        .unwrap();

        let merged = merge(vec![builtin, user]).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].desc, "user version");
        assert_eq!(merged[0].layer, Layer::User);
    }

    #[test]
    fn disable_patterns_hide_entries_from_earlier_layers() {
        let builtin = read_test(
            "version: 1
commands:
  - id: docker.ps
    cmd: docker ps
    desc: list containers
  - id: docker.logs
    cmd: docker logs
    desc: read logs
  - id: git.log
    cmd: git log
    desc: read history
",
            Layer::Builtin,
        )
        .unwrap();

        let user = read_test(
            "version: 1
disabled:
  - docker.*
",
            Layer::User,
        )
        .unwrap();

        let merged = merge(vec![builtin, user]).unwrap();
        assert_eq!(ids(&merged), ["git.log"]);
    }

    #[test]
    fn an_unsupported_schema_version_is_rejected() {
        let error = read_test("version: 99\ncommands: []\n", Layer::User).unwrap_err();
        assert!(error.to_string().contains("schema version 99"));
    }

    #[test]
    fn a_duplicate_id_within_one_file_is_rejected() {
        let error = read_test(
            "version: 1
commands:
  - id: dup
    cmd: a
    desc: first
  - id: dup
    cmd: b
    desc: second
",
            Layer::User,
        )
        .unwrap_err();
        assert!(error.to_string().contains("more than once"));
    }

    #[test]
    fn an_empty_description_is_rejected() {
        let error = read_test(
            "version: 1
commands:
  - id: bare
    cmd: ls
    desc: '  '
",
            Layer::User,
        )
        .unwrap_err();
        assert!(error.to_string().contains("empty description"));
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("lore-{}-{name}.yaml", std::process::id()));
        let _ = fs::remove_file(&path);
        path
    }

    fn new_entry(id: &str, cmd: &str) -> NewEntry {
        NewEntry {
            id: id.to_string(),
            cmd: cmd.to_string(),
            desc: "saved from the shell".to_string(),
            tags: vec!["saved".to_string()],
        }
    }

    #[test]
    fn appending_to_a_missing_file_writes_a_whole_document() {
        let path = scratch("missing");
        append(&path, &new_entry("user.kics", "kics scan -p .")).unwrap();

        let entries = load(Some(&path)).unwrap();
        let saved = entries.iter().find(|e| e.id == "user.kics").unwrap();
        assert_eq!(saved.cmd_for(ShellFamily::Posix), Some("kics scan -p ."));
        assert_eq!(saved.layer, Layer::User);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn appending_preserves_hand_written_comments() {
        let path = scratch("comments");
        fs::write(
            &path,
            "# my own notes, do not lose these\nversion: 1\ncommands:\n  - id: mine\n    cmd: ls\n    desc: list\n",
        )
        .unwrap();

        append(&path, &new_entry("user.added", "docker ps")).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my own notes, do not lose these"));

        let entries = load(Some(&path)).unwrap();
        let ids = ids(&entries);
        assert!(ids.contains(&"mine") && ids.contains(&"user.added"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn appended_commands_survive_yaml_quoting() {
        let path = scratch("quoting");
        let tricky = r#"docker ps --format "table {{.Names}}: #1" | grep -v '^x'"#;
        append(&path, &new_entry("user.tricky", tricky)).unwrap();

        let entries = load(Some(&path)).unwrap();
        let saved = entries.iter().find(|e| e.id == "user.tricky").unwrap();
        assert_eq!(saved.cmd_for(ShellFamily::Posix), Some(tricky));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn removing_an_entry_leaves_the_rest_of_the_file_alone() {
        let path = scratch("remove");
        fs::write(
            &path,
            "# notes I wrote myself\nversion: 1\ncommands:\n  - id: keep.me\n    cmd: ls\n    desc: list\n\n  - id: drop.me\n    cmd: rm -rf /\n    desc: do not\n  - id: keep.me.too\n    cmd: pwd\n    desc: where\n",
        )
        .unwrap();

        assert!(remove(&path, "drop.me").unwrap());

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# notes I wrote myself"));
        assert!(!text.contains("drop.me"));
        assert!(!text.contains("rm -rf"));

        let entries = load(Some(&path)).unwrap();
        let ids = ids(&entries);
        assert!(ids.contains(&"keep.me") && ids.contains(&"keep.me.too"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn removing_the_last_entry_does_not_swallow_what_follows() {
        let path = scratch("remove-last");
        fs::write(
            &path,
            "version: 1\ncommands:\n  - id: drop.me\n    cmd: ls\n    desc: list\ndisabled:\n  - docker.*\n",
        )
        .unwrap();

        assert!(remove(&path, "drop.me").unwrap());

        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("disabled:"),
            "lost the disabled list: {text:?}"
        );
        assert!(text.contains("docker.*"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn removing_something_that_is_not_there_reports_it() {
        let path = scratch("remove-missing");
        fs::write(&path, "version: 1\ncommands: []\n").unwrap();

        assert!(!remove(&path, "nope").unwrap());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn disabling_hides_a_builtin() {
        let path = scratch("disable");
        disable(&path, "docker.prune.all").unwrap();

        let entries = load(Some(&path)).unwrap();
        assert!(!ids(&entries).contains(&"docker.prune.all"));
        assert_eq!(entries.len(), 19);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn disabling_twice_extends_the_existing_list() {
        let path = scratch("disable-twice");
        disable(&path, "docker.prune.all").unwrap();
        disable(&path, "git.log.graph").unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("disabled:").count(), 1, "wrote {text:?}");

        let entries = load(Some(&path)).unwrap();
        assert_eq!(entries.len(), 18);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn disabling_leaves_saved_commands_in_place() {
        let path = scratch("disable-keeps");
        append(&path, &new_entry("user.mine", "docker ps")).unwrap();
        disable(&path, "docker.prune.all").unwrap();

        let entries = load(Some(&path)).unwrap();
        let ids = ids(&entries);
        assert!(ids.contains(&"user.mine"));
        assert!(!ids.contains(&"docker.prune.all"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn suggested_ids_read_like_the_command_and_avoid_collisions() {
        let mut taken = BTreeSet::new();
        assert_eq!(suggest_id("docker ps -a", &taken), "user.docker-ps");

        taken.insert("user.docker-ps".to_string());
        assert_eq!(suggest_id("docker ps -a", &taken), "user.docker-ps-2");

        taken.insert("user.docker-ps-2".to_string());
        assert_eq!(suggest_id("docker ps -a", &taken), "user.docker-ps-3");
    }

    #[test]
    fn a_suggested_id_ignores_leading_flags_and_odd_input() {
        let taken = BTreeSet::new();
        assert_eq!(
            suggest_id("kubectl -n prod get", &taken),
            "user.kubectl-prod"
        );
        assert_eq!(suggest_id("--- ???", &taken), "user.command");
        assert_eq!(suggest_id("", &taken), "user.command");
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let error = read_test(
            "version: 1
commands:
  - id: typo
    cmd: ls
    desc: list
    tag: [files]
",
            Layer::User,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not a valid command library"));
    }
}
