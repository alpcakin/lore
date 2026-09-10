//! On-disk locations and readers for everything lore persists.

pub mod definitions;
pub mod stats;

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

/// Overrides for the two directories lore writes to.
///
/// A portable install keeps everything beside the binary, and a test needs a
/// directory of its own. `ProjectDirs` cannot be redirected on Windows, so the
/// override has to sit in front of it rather than inside it.
const CONFIG_DIR: &str = "LORE_CONFIG_DIR";
const DATA_DIR: &str = "LORE_DATA_DIR";

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("", "", "lore").context("could not determine this platform's home directory")
}

fn overridden(variable: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The user's editable command library.
///
/// Kept in the config directory because users are expected to track it in their
/// own git repository. Usage statistics deliberately live somewhere else so that
/// syncing definitions never produces churn or merge conflicts.
pub fn user_library() -> Result<PathBuf> {
    let directory = match overridden(CONFIG_DIR) {
        Some(directory) => directory,
        None => project_dirs()?.config_dir().to_path_buf(),
    };

    Ok(directory.join("commands.yaml"))
}

/// The usage statistics database.
///
/// Machine local and never synced.
pub fn stats_database() -> Result<PathBuf> {
    let directory = match overridden(DATA_DIR) {
        Some(directory) => directory,
        None => project_dirs()?.data_dir().to_path_buf(),
    };

    Ok(directory.join("stats.db"))
}
