//! Responsibility: maps the project `.yaml` document onto the rig it describes.
//!
//! Owns the `serde_yaml` <-> [`RigProject`] boundary. The document is wrapped
//! under a top-level `project:` key. Parsing validates via
//! [`RigProject::validate`]; round-trip is deterministic (`BTreeMap` ordering).

use anyhow::{anyhow, Context, Result};
use project::format_version::{project_format_version, MAX_READABLE_FORMAT_VERSION};
use project::rig::RigProject;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Missing `version:` ⇒ a pre-version file, whose shape *is* v1.
fn default_doc_version() -> u32 {
    1
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectFile {
    #[serde(default = "default_doc_version")]
    version: u32,
    project: RigProject,
}

/// Parse + validate a project document from a YAML string.
///
/// The `version:` field gates compatibility: a newer document is refused
/// cleanly (rather than silently dropping unknown fields); an older one is
/// staged-upgraded in memory (no upgrades exist for v1 yet).
pub fn parse_project(yaml: &str) -> Result<RigProject> {
    let file: ProjectFile = serde_yaml::from_str(yaml).context("failed to parse project")?;
    if file.version > MAX_READABLE_FORMAT_VERSION {
        return Err(anyhow!(
            "project version {} is newer than this build supports \
             (max {MAX_READABLE_FORMAT_VERSION}); please upgrade OpenRig",
            file.version
        ));
    }
    // version < CURRENT ⇒ staged in-memory upgrades would run here.
    file.project
        .validate()
        .map_err(|e| anyhow!("invalid project: {e}"))?;
    Ok(file.project)
}

/// Serialize a [`RigProject`] to the project YAML, stamping the format
/// version its content needs: `2` only when a preset holds a split, so a
/// split-free project stays readable by builds that predate the split.
pub fn serialize_project(project: &RigProject) -> Result<String> {
    let file = ProjectFile {
        version: project_format_version(project),
        project: project.clone(),
    };
    serde_yaml::to_string(&file).context("failed to serialize project")
}

/// Load + validate a project file from disk.
pub fn load_project_file(path: &Path) -> Result<RigProject> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_project(&raw).with_context(|| format!("failed to load project {}", path.display()))
}

/// Serialize and write a [`RigProject`] to disk (creating parent dirs).
pub fn save_project_file(path: &Path, project: &RigProject) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, serialize_project(project)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "project_file_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "project_file_fields_tests.rs"]
mod fields_tests;
