//! Project bundle — the on-disk representation of a Scorestrata project.

use std::path::{Path, PathBuf};

use scorestrata_core::domain::Project;
use scorestrata_core::error::ScorestrataError;

/// The file extension for project bundles.
pub const PROJECT_EXT: &str = "ssproj";

/// Load a project from a directory path.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the project file cannot be read,
/// or [`ScorestrataError::Serialization`] if the JSON is invalid.
pub fn load_project(path: &Path) -> Result<Project, ScorestrataError> {
    let project_file = path.join("project.json");
    let data = std::fs::read_to_string(&project_file).map_err(|e| ScorestrataError::Io {
        path: project_file.display().to_string(),
        source: e,
    })?;
    serde_json::from_str(&data).map_err(|e| ScorestrataError::Serialization {
        message: format!("Failed to deserialize project: {e}"),
    })
}

/// Save a project to a directory path.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the directory or file cannot be written,
/// or [`ScorestrataError::Serialization`] if serialization fails.
pub fn save_project(project: &Project, path: &Path) -> Result<(), ScorestrataError> {
    std::fs::create_dir_all(path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let project_file = path.join("project.json");
    let data =
        serde_json::to_string_pretty(project).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to serialize project: {e}"),
        })?;
    std::fs::write(&project_file, data).map_err(|e| ScorestrataError::Io {
        path: project_file.display().to_string(),
        source: e,
    })
}

/// Create a new empty project directory structure.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if any directory cannot be created.
pub fn init_project_bundle(path: &Path) -> Result<(), ScorestrataError> {
    let dirs = [
        "patterns",
        "segments",
        "snapshots",
        "branches",
        "ledger",
        "memory",
        "evidence",
        "cache",
    ];
    for dir in &dirs {
        std::fs::create_dir_all(path.join(dir)).map_err(|e| ScorestrataError::Io {
            path: path.join(dir).display().to_string(),
            source: e,
        })?;
    }
    Ok(())
}

/// Get the path to a snapshot file within a project bundle.
#[must_use]
pub fn snapshot_path(project_path: &Path, snapshot_id: &str) -> PathBuf {
    project_path
        .join("snapshots")
        .join(format!("{snapshot_id}.json"))
}

/// Get the path to the event ledger.
#[must_use]
pub fn ledger_path(project_path: &Path) -> PathBuf {
    project_path.join("ledger").join("events.jsonl")
}
