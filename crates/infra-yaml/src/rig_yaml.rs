//! `project.openrig` file parser/serializer (#449).
//!
//! Owns the `serde_yaml` <-> [`RigProject`] boundary. The document is wrapped
//! under a top-level `project:` key. Parsing validates via
//! [`RigProject::validate`]; round-trip is deterministic (`BTreeMap` ordering).

use anyhow::{anyhow, Context, Result};
use project::rig::RigProject;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
struct RigProjectFile {
    project: RigProject,
}

/// Parse + validate a `project.openrig` document from a YAML string.
pub fn parse_rig_project(yaml: &str) -> Result<RigProject> {
    let file: RigProjectFile =
        serde_yaml::from_str(yaml).context("failed to parse project.openrig")?;
    file.project
        .validate()
        .map_err(|e| anyhow!("invalid project.openrig: {e}"))?;
    Ok(file.project)
}

/// Serialize a [`RigProject`] back to a `project.openrig` YAML string.
pub fn serialize_rig_project(project: &RigProject) -> Result<String> {
    let file = RigProjectFile {
        project: project.clone(),
    };
    serde_yaml::to_string(&file).context("failed to serialize project.openrig")
}

/// Load + validate a `project.openrig` file from disk.
pub fn load_rig_project_file(path: &Path) -> Result<RigProject> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_rig_project(&raw)
}

/// Serialize and write a [`RigProject`] to disk (creating parent dirs).
pub fn save_rig_project_file(path: &Path, project: &RigProject) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create {}", parent.display())
        })?;
    }
    fs::write(path, serialize_rig_project(project)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "rig_yaml_tests.rs"]
mod tests;
