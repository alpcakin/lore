//! Reading command definition files and merging them into one library.
//!
//! Layers are merged weakest first, so a later layer shadows an earlier one by
//! entry id. Any layer may also hide entries by glob pattern without redefining
//! them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::model::{Entry, Layer, Library};

/// Schema version this build understands.
const SCHEMA_VERSION: u32 = 1;

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
