//! On-disk locations and readers for everything lore persists.

pub mod definitions;
pub mod stats;

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("", "", "lore").context("could not determine this platform's home directory")
}

/// The user's editable command library.
///
/// Kept in the config directory because users are expected to track it in their
/// own git repository. Usage statistics deliberately live somewhere else so that
/// syncing definitions never produces churn or merge conflicts.
pub fn user_library() -> Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("commands.yaml"))
}

/// The usage statistics database.
///
/// Machine local and never synced.
pub fn stats_database() -> Result<PathBuf> {
    Ok(project_dirs()?.data_dir().join("stats.db"))
}
