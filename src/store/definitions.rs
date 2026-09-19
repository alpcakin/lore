//! Reading command definition files and merging them into one library.
//!
//! Layers are merged weakest first, so a later layer shadows an earlier one by
//! entry id. Any layer may also hide entries by glob pattern without redefining
//! them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::ops::Range;
use std::path::Path;

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;

use crate::model::{CommandBody, Entry, Layer, Library, ParamSpec};

/// Schema version this build understands.
const SCHEMA_VERSION: u32 = 1;

/// Top level key holding the ids a layer hides.
const DISABLED: &str = "disabled:";

/// Top level key holding a layer's entries.
const COMMANDS: &str = "commands:";

/// How far list items are indented in a file that has none yet.
const DEFAULT_INDENT: &str = "  ";

macro_rules! builtin {
    ($name:literal) => {
        (
            concat!("builtin:", $name, ".yaml"),
            include_str!(concat!("../../assets/builtins/", $name, ".yaml")),
        )
    };
}

/// Libraries compiled into the binary, so a fresh install opens onto a full
/// picker without a network round trip.
///
/// One file per namespace, listed by hand rather than gathered by a build
/// script: a list read as easily as it is written is worth more here than one
/// that maintains itself.
const BUILTINS: &[(&str, &str)] = &[
    builtin!("archive"),
    builtin!("docker"),
    builtin!("git"),
    builtin!("kubernetes"),
    builtin!("network"),
    builtin!("node"),
    builtin!("python"),
    builtin!("rust"),
    builtin!("ssh"),
    builtin!("system"),
    builtin!("text"),
];

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
///
/// Carries every field an entry can hold, not only the ones a form asks for.
/// Rewriting an existing entry serialises this whole struct, so anything left
/// out here would be dropped from the file the moment it was edited.
#[derive(Debug, Serialize)]
pub struct NewEntry {
    pub id: String,
    pub cmd: CommandBody,
    pub desc: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, ParamSpec>,

    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub danger: bool,
}

/// Splits the comma separated tags a user typed into a clean list.
pub fn parse_tags(text: &str) -> Vec<String> {
    text.split(',')
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect()
}

/// Separates `#tags` from the answer to "what is it for?".
///
/// Saving asks one question rather than filling in a form, so tags ride along
/// in the answer the way they do in a commit message or a post: any word that
/// starts with `#` becomes a tag and the rest is the description.
pub fn split_purpose(text: &str) -> (String, Vec<String>) {
    let mut words = Vec::new();
    let mut tags = Vec::new();

    for word in text.split_whitespace() {
        match word.strip_prefix('#') {
            Some(tag) if !tag.is_empty() => {
                let tag = tag.to_lowercase();
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
            _ => words.push(word),
        }
    }

    (words.join(" "), tags)
}

/// Words to know a command by, taken from the command itself.
///
/// The program and the subcommands that follow it, stopping at the first flag,
/// placeholder, path or quote: `kubectl logs -f <pod>` gives `kubectl` and
/// `logs`. That is what someone types when they half remember the command, and
/// it saves them inventing tags for every entry.
pub fn derive_tags(cmd: &str) -> Vec<String> {
    /// Deep enough for `docker compose logs`, short of the arguments.
    const DEPTH: usize = 3;

    let mut words = cmd
        .split_whitespace()
        .skip_while(|word| matches!(*word, "sudo" | "doas") || word.contains('='));

    let mut tags: Vec<String> = Vec::new();

    // The program may be named by path, and the name is the part worth keeping.
    if let Some(program) = words.next() {
        let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
        let name = name.strip_suffix(".exe").unwrap_or(name).to_lowercase();
        if plain(&name) {
            tags.push(name);
        } else {
            return tags;
        }
    }

    for word in words {
        if tags.len() == DEPTH || !plain(word) {
            break;
        }
        if !tags.iter().any(|tag| tag == word) {
            tags.push(word.to_string());
        }
    }

    tags
}

/// A word that reads as a name: lower case letters, digits and hyphens,
/// starting with a letter, at least two long.
fn plain(word: &str) -> bool {
    word.len() >= 2
        && word.starts_with(|c: char| c.is_ascii_lowercase())
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Tags the user gave first, then the ones the command suggests.
pub fn merge_tags(given: Vec<String>, cmd: &str) -> Vec<String> {
    let mut tags = given;
    for tag in derive_tags(cmd) {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

/// What `upsert` did to the file.
#[derive(Debug, PartialEq, Eq)]
pub enum Written {
    Replaced,
    Appended,
}

/// Appends an entry to the user's library, creating the file if needed.
///
/// The entry is added as text rather than by re-serialising the whole
/// document, because users are told to hand edit and version this file. Round
/// tripping it through a parser would silently delete their comments and
/// reflow everything they had arranged.
pub fn append(path: &Path, entry: &NewEntry) -> Result<()> {
    let text = read_or_empty(path)?;
    write(path, &appended(&text, entry)?)
}

/// `text` with `entry` added as the last item of its `commands` list.
///
/// The last item of the list, not the last line of the file. A library whose
/// `disabled` list comes after its commands, which is what hiding a builtin
/// produces, would otherwise take the new entry into the disabled list and
/// stop parsing.
pub fn appended(text: &str, entry: &NewEntry) -> Result<String> {
    if text.trim().is_empty() {
        return Ok(format!(
            "version: {SCHEMA_VERSION}\n{COMMANDS}\n{}",
            as_list_item(entry, DEFAULT_INDENT)?
        ));
    }

    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let Some(key) = open_list(&mut lines, COMMANDS)? else {
        lines.push(COMMANDS.to_string());
        let mut out = joined(&lines);
        out.push_str(&as_list_item(entry, DEFAULT_INDENT)?);
        return Ok(out);
    };

    let end = end_of_section(&lines, key);
    let indent = item_indent(&lines[key + 1..end]).unwrap_or(DEFAULT_INDENT.to_string());
    let item = as_list_item(entry, &indent)?;

    // After the last line that belongs to the list, so blank lines and a
    // comment introducing whatever comes next stay with it.
    let mut at = end;
    while at > key + 1 && belongs_to_next(&lines[at - 1]) {
        at -= 1;
    }
    lines.splice(at..at, item.lines().map(String::from));

    Ok(joined(&lines))
}

/// Renders one entry as a YAML list item indented by `indent`.
///
/// The body is produced by the serialiser so that quoting and escaping are
/// correct, then indented into place.
fn as_list_item(entry: &NewEntry, indent: &str) -> Result<String> {
    let body = serde_yaml_ng::to_string(entry).context("failed to serialise the entry")?;

    let mut out = String::new();
    for (index, line) in body.lines().enumerate() {
        out.push_str(indent);
        out.push_str(if index == 0 { "- " } else { "  " });
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

    match removed(&text, id) {
        Some(text) => {
            write(path, &text)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// `text` without the entry declared as `id`, or `None` if there is none.
pub fn removed(text: &str, id: &str) -> Option<String> {
    item_of(text, id).map(|block| splice(text, block, ""))
}

/// Writes an entry into the user's library, replacing one already declared
/// under the same id.
///
/// Nothing outside the one list item is touched, so the comments a user wrote
/// around their entries survive. A comment sitting inside the entry being
/// rewritten does not, which is the price of not reserialising the document.
///
/// A builtin cannot be changed where it lives, inside the binary. Writing it
/// here under its own id is enough: the loader shadows by id, so the user's
/// copy wins.
pub fn upsert(path: &Path, entry: &NewEntry) -> Result<Written> {
    let text = read_or_empty(path)?;
    let (text, written) = upserted(&text, entry)?;
    write(path, &text)?;
    Ok(written)
}

/// `text` with `entry` in place of the one declared under its id, or added to
/// the end of the list when there is none.
pub fn upserted(text: &str, entry: &NewEntry) -> Result<(String, Written)> {
    match item_of(text, &entry.id) {
        Some(block) => {
            let lines: Vec<&str> = text.lines().collect();
            let indent = lines[block.start][..indent_of(lines[block.start])].to_string();
            let item = as_list_item(entry, &indent)?;
            Ok((splice(text, block, &item), Written::Replaced))
        }
        None => Ok((appended(text, entry)?, Written::Appended)),
    }
}

/// Swaps the lines of one list item for `replacement`, which may be empty.
fn splice(text: &str, block: Range<usize>, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len() + replacement.len());

    for (number, line) in text.lines().enumerate() {
        if number == block.start {
            out.push_str(replacement);
        }
        if !block.contains(&number) {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

fn read_or_empty(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

/// Hides an entry the user cannot delete, such as one compiled into the binary.
pub fn disable(path: &Path, id: &str) -> Result<()> {
    let text = read_or_empty(path)?;
    write(path, &with_disabled(&text, id)?)
}

/// `text` with `pattern` added to its `disabled` list.
pub fn with_disabled(text: &str, pattern: &str) -> Result<String> {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();

    match open_list(&mut lines, DISABLED)? {
        Some(key) => {
            let end = end_of_section(&lines, key);
            let indent = item_indent(&lines[key + 1..end]).unwrap_or(DEFAULT_INDENT.to_string());
            lines.insert(key + 1, format!("{indent}- {pattern}"));
        }
        None => {
            if text.trim().is_empty() {
                lines.push(format!("version: {SCHEMA_VERSION}"));
            }
            lines.push(DISABLED.to_string());
            lines.push(format!("{DEFAULT_INDENT}- {pattern}"));
        }
    }

    Ok(joined(&lines))
}

/// `text` with `pattern` taken off its `disabled` list, if it was on it.
pub fn with_enabled(text: &str, pattern: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let Some(key) = lines.iter().position(|line| is_key(line, DISABLED)) else {
        return text.to_string();
    };
    let end = end_of_section_of(&lines, key);

    let kept: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(number, line)| {
            !(*number > key
                && *number < end
                && line
                    .trim()
                    .strip_prefix("- ")
                    .is_some_and(|value| unquote(value.trim()) == pattern))
        })
        .map(|(_, line)| line.to_string())
        .collect();

    joined(&kept)
}

/// Where the list under the top level `key` starts, turning `key: []` into an
/// open list first so that items can go under it.
fn open_list(lines: &mut [String], key: &str) -> Result<Option<usize>> {
    let Some(at) = lines.iter().position(|line| is_key(line, key)) else {
        return Ok(None);
    };

    let rest = lines[at][key.len()..].trim();
    // A comment after the key says nothing about the list.
    let rest = if rest.starts_with('#') { "" } else { rest };
    match rest {
        "" => {}
        "[]" => lines[at] = key.to_string(),
        _ => bail!(
            "`{}` is written on one line, add to it by hand",
            lines[at].trim()
        ),
    }
    Ok(Some(at))
}

/// Whether `line` is the top level `key`, such as `commands:`.
fn is_key(line: &str, key: &str) -> bool {
    line.starts_with(key)
}

/// The line after the last one belonging to the section opened at `key`.
fn end_of_section(lines: &[String], key: usize) -> usize {
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    end_of_section_of(&lines, key)
}

fn end_of_section_of(lines: &[&str], key: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(key + 1)
        .find(|(_, line)| starts_top_level(line))
        .map_or(lines.len(), |(at, _)| at)
}

/// A line that opens a new top level key rather than continuing a list.
///
/// A list may be written flush with the margin, so a line starting with `-`
/// still belongs to it, as does a comment.
fn starts_top_level(line: &str) -> bool {
    !line.is_empty() && indent_of(line) == 0 && !line.starts_with('-') && !line.starts_with('#')
}

/// Blank lines and margin comments at the end of a section introduce what
/// follows it rather than closing what came before.
fn belongs_to_next(line: &str) -> bool {
    line.trim().is_empty() || line.starts_with('#')
}

/// How far the list's items are indented, from the first one there is.
fn item_indent<S: AsRef<str>>(lines: &[S]) -> Option<String> {
    lines
        .iter()
        .map(AsRef::as_ref)
        .find(|line| line.trim_start().starts_with("- "))
        .map(|line| line[..indent_of(line)].to_string())
}

fn joined<S: AsRef<str>>(lines: &[S]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(line.as_ref());
        out.push('\n');
    }
    out
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
        assert!(
            entries.len() > 100,
            "only {} entries shipped",
            entries.len()
        );
        assert!(entries.iter().all(|e| e.layer == Layer::Builtin));
    }

    /// Ids are the only handle a user, an override and the statistics all share,
    /// so a duplicate across two namespace files would silently shadow an entry.
    #[test]
    fn builtin_ids_are_unique_and_namespaced() {
        let entries = load(None).unwrap();
        let mut seen = BTreeSet::new();

        for entry in &entries {
            assert!(seen.insert(entry.id.clone()), "{} appears twice", entry.id);

            let (namespace, rest) = entry.id.split_once('.').unwrap_or((&entry.id, ""));
            assert!(
                !namespace.is_empty() && !rest.is_empty(),
                "{} is not namespaced",
                entry.id
            );
            assert!(
                entry
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'),
                "{} is not a plain id",
                entry.id
            );
        }
    }

    /// A placeholder with no description is a prompt with nothing above it. The
    /// user is being asked for a value and told nothing about what it is.
    #[test]
    fn every_builtin_placeholder_is_documented() {
        let entries = load(None).unwrap();

        for entry in &entries {
            for family in [ShellFamily::Posix, ShellFamily::PowerShell] {
                let Some(cmd) = entry.cmd_for(family) else {
                    continue;
                };

                for name in crate::params::names(cmd) {
                    let documented = entry
                        .params
                        .get(&name)
                        .is_some_and(|spec| spec.desc.is_some());
                    assert!(documented, "{} does not document <{name}>", entry.id);
                }
            }
        }
    }

    /// A description is the only thing most searches match against, and a tag
    /// list is what makes an entry findable under a word it does not contain.
    #[test]
    fn every_builtin_is_findable() {
        let entries = load(None).unwrap();

        for entry in &entries {
            assert!(
                !entry.desc.trim().is_empty(),
                "{} has no description",
                entry.id
            );
            assert!(entry.tags.len() >= 2, "{} carries too few tags", entry.id);
        }
    }

    /// The interface is ASCII only: a legacy Windows console runs on the system
    /// code page, where anything else arrives as mojibake.
    fn echo_entry(id: &str) -> NewEntry {
        new_entry(id, &format!("echo {id}"))
    }

    fn ids_in(text: &str) -> Vec<String> {
        read(text, "test", Layer::User)
            .expect("the text should still be a valid library")
            .commands
            .into_iter()
            .map(|entry| entry.id)
            .collect()
    }

    /// What every released version up to 0.1.2 did: hiding a builtin puts a
    /// disabled list at the end of the file, and the next save went into it.
    #[test]
    fn a_new_entry_goes_into_the_commands_list_even_when_it_is_not_last() {
        let text = "version: 1\ncommands:\n  - id: one\n    cmd: echo one\n    desc: One\ndisabled:\n  - git.status\n";
        let text = appended(text, &echo_entry("two")).unwrap();

        assert_eq!(ids_in(&text), ["one", "two"]);
        assert!(
            text.ends_with("disabled:\n  - git.status\n"),
            "wrote {text}"
        );
    }

    #[test]
    fn a_comment_introducing_the_next_section_stays_with_it() {
        let text = "version: 1\ncommands:\n  - id: one\n    cmd: echo one\n    desc: One\n\n# builtins I never use\ndisabled:\n  - docker.*\n";
        let text = appended(text, &echo_entry("two")).unwrap();

        assert_eq!(ids_in(&text), ["one", "two"]);
        assert!(
            text.contains("- saved\n\n# builtins I never use\ndisabled:"),
            "wrote {text}"
        );
    }

    #[test]
    fn a_list_written_flush_with_the_margin_stays_that_way() {
        let text = "version: 1\ncommands:\n- id: one\n  cmd: echo one\n  desc: One\n";
        let text = appended(text, &echo_entry("two")).unwrap();

        assert_eq!(ids_in(&text), ["one", "two"]);
        assert!(text.contains("\n- id: two\n"), "wrote {text}");
    }

    #[test]
    fn an_empty_inline_list_is_opened_up() {
        let text = appended("version: 1\ncommands: []\n", &echo_entry("one")).unwrap();
        assert_eq!(ids_in(&text), ["one"]);
    }

    #[test]
    fn a_file_with_no_commands_key_gains_one() {
        let text = appended("version: 1\ndisabled:\n  - x\n", &echo_entry("one")).unwrap();
        assert_eq!(ids_in(&text), ["one"]);
    }

    #[test]
    fn replacing_keeps_the_indentation_the_file_uses() {
        let text = "version: 1\ncommands:\n- id: one\n  cmd: echo one\n  desc: One\n";
        let mut entry = echo_entry("one");
        entry.desc = "Changed".to_string();

        let (text, written) = upserted(text, &entry).unwrap();
        assert_eq!(written, Written::Replaced);
        assert!(text.contains("\n- id: one\n"), "wrote {text}");
        assert!(text.contains("desc: Changed"), "wrote {text}");
    }

    #[test]
    fn hiding_then_unhiding_leaves_the_file_as_it_was() {
        let original = "version: 1\ncommands:\n  - id: one\n    cmd: echo one\n    desc: One\ndisabled:\n  - docker.*\n";
        let hidden = with_disabled(original, "git.status").unwrap();
        assert!(hidden.contains("  - git.status\n"), "wrote {hidden}");

        assert_eq!(with_enabled(&hidden, "git.status"), original);
    }

    #[test]
    fn unhiding_something_never_hidden_changes_nothing() {
        let original =
            "version: 1\ncommands:\n  - id: git.status\n    cmd: git status\n    desc: S\n";
        assert_eq!(with_enabled(original, "git.status"), original);
    }

    #[test]
    fn hashtags_in_the_purpose_become_tags() {
        assert_eq!(
            split_purpose("Follow the api logs #k8s #Debug"),
            (
                "Follow the api logs".to_string(),
                vec!["k8s".to_string(), "debug".to_string()]
            )
        );
        assert_eq!(
            split_purpose("  Tail   the #logs log  "),
            ("Tail the log".to_string(), vec!["logs".to_string()])
        );
    }

    /// A lone `#` is punctuation, not an empty tag.
    #[test]
    fn a_bare_hash_stays_in_the_description() {
        assert_eq!(
            split_purpose("Issue # 42 fix"),
            ("Issue # 42 fix".to_string(), Vec::new())
        );
    }

    #[test]
    fn tags_come_from_the_program_and_its_subcommands() {
        assert_eq!(derive_tags("kubectl logs -f <pod>"), ["kubectl", "logs"]);
        assert_eq!(
            derive_tags("docker compose logs -f api"),
            ["docker", "compose", "logs"]
        );
        assert_eq!(
            derive_tags("git log -S\"<text>\" --oneline"),
            ["git", "log"]
        );
        assert_eq!(derive_tags("ls -lah"), ["ls"]);
    }

    #[test]
    fn tags_skip_what_is_not_the_program() {
        assert_eq!(
            derive_tags("sudo systemctl restart nginx"),
            ["systemctl", "restart", "nginx"]
        );
        assert_eq!(derive_tags("RUST_LOG=debug cargo run"), ["cargo", "run"]);
        assert_eq!(
            derive_tags("/usr/local/bin/terraform plan"),
            ["terraform", "plan"]
        );
        assert!(derive_tags("./deploy.sh prod").is_empty());
        assert!(derive_tags("").is_empty());
    }

    #[test]
    fn given_tags_come_first_and_are_not_repeated() {
        assert_eq!(
            merge_tags(
                vec!["k8s".to_string(), "logs".to_string()],
                "kubectl logs -f x"
            ),
            ["k8s", "logs", "kubectl"]
        );
    }

    #[test]
    fn the_builtin_library_is_ascii() {
        for (origin, source) in BUILTINS {
            assert!(source.is_ascii(), "{origin} is not ascii");
        }
    }

    #[test]
    fn builtin_library_carries_working_placeholders() {
        let entries = load(None).unwrap();
        let logs = entries
            .iter()
            .find(|e| e.id == "docker.logs")
            .expect("docker.logs must exist");

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

        assert!(ports.cmd_for(ShellFamily::Posix).unwrap().contains("lsof "));
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
            cmd: CommandBody::Shared(cmd.to_string()),
            desc: "saved from the shell".to_string(),
            tags: vec!["saved".to_string()],
            params: BTreeMap::new(),
            danger: false,
        }
    }

    /// The whole reason an entry is spliced rather than the document
    /// reserialised: a user's own file is something they wrote, and a save must
    /// not reflow it.
    #[test]
    fn rewriting_an_entry_leaves_the_rest_of_the_file_alone() {
        let path = scratch("rewrite");
        fs::write(
            &path,
            concat!(
                "version: 1\n",
                "# my own commands\n",
                "commands:\n",
                "\n",
                "  # the one I always forget\n",
                "  - id: user.kics\n",
                "    cmd: kics scan -p .\n",
                "    desc: old\n",
                "\n",
                "  - id: user.trivy\n",
                "    cmd: trivy image alpine\n",
                "    desc: keep me\n",
            ),
        )
        .unwrap();

        let mut entry = new_entry("user.kics", "kics scan -p . --report-formats json");
        entry.desc = "new".to_string();
        assert_eq!(upsert(&path, &entry).unwrap(), Written::Replaced);

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my own commands"), "wrote {text:?}");
        assert!(text.contains("# the one I always forget"), "wrote {text:?}");
        assert!(text.contains("desc: keep me"), "wrote {text:?}");
        assert!(text.contains("--report-formats json"), "wrote {text:?}");
        assert!(!text.contains("desc: old"), "wrote {text:?}");
        assert_eq!(text.matches("id: user.kics").count(), 1, "wrote {text:?}");
    }

    /// A form only ever shows the command, the description and the tags. Every
    /// other field has to survive an edit that never mentioned it.
    #[test]
    fn rewriting_keeps_the_fields_no_form_ever_shows() {
        let path = scratch("rewrite-fields");
        let entry = NewEntry {
            id: "sys.ports".to_string(),
            cmd: CommandBody::PerShell(BTreeMap::from([
                (ShellFamily::Posix, "ss -tulpn".to_string()),
                (ShellFamily::PowerShell, "Get-NetTCPConnection".to_string()),
            ])),
            desc: "List listening ports".to_string(),
            tags: vec!["net".to_string()],
            params: BTreeMap::from([(
                "port".to_string(),
                ParamSpec {
                    desc: Some("Port to look for".to_string()),
                    from: None,
                },
            )]),
            danger: true,
        };

        assert_eq!(upsert(&path, &entry).unwrap(), Written::Appended);
        let reloaded = load(Some(&path)).unwrap();
        let reloaded = reloaded
            .iter()
            .find(|e| e.id == "sys.ports")
            .expect("the entry should load back");

        assert_eq!(reloaded.cmd_for(ShellFamily::Posix), Some("ss -tulpn"));
        assert_eq!(
            reloaded.cmd_for(ShellFamily::PowerShell),
            Some("Get-NetTCPConnection")
        );
        assert!(reloaded.danger);
        assert_eq!(
            reloaded.params["port"].desc.as_deref(),
            Some("Port to look for")
        );
    }

    /// A builtin cannot be rewritten where it lives. Writing it under its own id
    /// is enough because the loader shadows by id.
    #[test]
    fn upserting_an_id_the_file_does_not_hold_appends_it() {
        let path = scratch("upsert-new");
        fs::write(
            &path,
            concat!(
                "version: 1\n",
                "commands:\n",
                "  - id: user.trivy\n",
                "    cmd: trivy image alpine\n",
                "    desc: keep me\n",
            ),
        )
        .unwrap();

        assert_eq!(
            upsert(&path, &new_entry("git.log.graph", "git log --graph")).unwrap(),
            Written::Appended
        );

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("desc: keep me"), "wrote {text:?}");
        assert!(text.contains("id: git.log.graph"), "wrote {text:?}");
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
        let before = load(None).unwrap().len();
        disable(&path, "docker.prune.everything").unwrap();

        let entries = load(Some(&path)).unwrap();
        assert!(!ids(&entries).contains(&"docker.prune.everything"));
        assert_eq!(entries.len(), before - 1);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn disabling_twice_extends_the_existing_list() {
        let path = scratch("disable-twice");
        let before = load(None).unwrap().len();
        disable(&path, "docker.prune.everything").unwrap();
        disable(&path, "git.log.graph").unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("disabled:").count(), 1, "wrote {text:?}");

        let entries = load(Some(&path)).unwrap();
        assert_eq!(entries.len(), before - 2);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn disabling_leaves_saved_commands_in_place() {
        let path = scratch("disable-keeps");
        append(&path, &new_entry("user.mine", "docker ps")).unwrap();
        disable(&path, "docker.prune.everything").unwrap();

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
