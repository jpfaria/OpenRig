//! `project.openrig` file parser/serializer (#449).
//!
//! Owns the `serde_yaml` <-> [`RigProject`] boundary. The document is wrapped
//! under a top-level `project:` key. Parsing validates via
//! [`RigProject::validate`]; round-trip is deterministic (`BTreeMap` ordering).

use crate::YamlProjectRepository;
use anyhow::{anyhow, Context, Result};
use project::migrate::migrate_legacy_project;
use project::rig::{RigProject, PROJECT_FORMAT_VERSION};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Missing `version:` ⇒ a pre-version file, whose shape *is* v1.
fn default_doc_version() -> u32 {
    1
}

#[derive(Debug, Serialize, Deserialize)]
struct RigProjectFile {
    #[serde(default = "default_doc_version")]
    version: u32,
    project: RigProject,
}

/// Parse + validate a `project.openrig` document from a YAML string.
///
/// The `version:` field gates compatibility: a newer document is refused
/// cleanly (rather than silently dropping unknown fields); an older one is
/// staged-upgraded in memory (no upgrades exist for v1 yet).
pub fn parse_rig_project(yaml: &str) -> Result<RigProject> {
    let file: RigProjectFile =
        serde_yaml::from_str(yaml).context("failed to parse project.openrig")?;
    if file.version > PROJECT_FORMAT_VERSION {
        return Err(anyhow!(
            "project.openrig version {} is newer than this build supports \
             (max {PROJECT_FORMAT_VERSION}); please upgrade OpenRig",
            file.version
        ));
    }
    // version < CURRENT ⇒ staged in-memory upgrades would run here.
    file.project
        .validate()
        .map_err(|e| anyhow!("invalid project.openrig: {e}"))?;
    Ok(file.project)
}

/// Serialize a [`RigProject`] back to a `project.openrig` YAML string,
/// stamping the current format version.
pub fn serialize_rig_project(project: &RigProject) -> Result<String> {
    let file = RigProjectFile {
        version: PROJECT_FORMAT_VERSION,
        project: project.clone(),
    };
    serde_yaml::to_string(&file).context("failed to serialize project.openrig")
}

/// Load + validate a `project.openrig` file from disk.
pub fn load_rig_project_file(path: &Path) -> Result<RigProject> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_rig_project(&raw)
}

/// Serialize and write a [`RigProject`] to disk (creating parent dirs).
pub fn save_rig_project_file(path: &Path, project: &RigProject) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, serialize_rig_project(project)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Transparent project loader: returns a [`RigProject`] regardless of the
/// on-disk format. A new `project.openrig` is parsed (version-checked) as-is;
/// a legacy chain YAML is migrated on the spot to a sibling
/// `project.openrig` (with a one-time `<legacy>.bak`), idempotently. The
/// caller never has to know which format it was.
pub fn load_project_any(path: &Path) -> Result<RigProject> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    // New format ⇔ it structurally deserializes into the versioned wrapper
    // (a legacy doc has `chains:` and no `project:` key, so this fails and
    // we fall through to migration).
    if serde_yaml::from_str::<RigProjectFile>(&raw).is_ok() {
        return parse_rig_project(&raw);
    }
    let out_path = path.with_extension("openrig");
    migrate_legacy_project_file(path, &out_path)
}

/// Migrate a legacy chain-based project file (`*.yaml`) into a
/// `project.openrig` at `out_path`. Idempotent and safe:
///
/// - if `out_path` already holds a valid `RigProject`, it is returned as-is
///   (the legacy file is **not** re-read and the target is **not** clobbered);
/// - the legacy file is backed up to `<legacy>.bak` exactly once (skipped if
///   the backup already exists), before anything is written;
/// - the migrated project is validated before being written.
///
/// Audio is unchanged: [`migrate_legacy_project`] preserves processing blocks
/// bit-identical and carries `Chain.volume`.
pub fn migrate_legacy_project_file(legacy_path: &Path, out_path: &Path) -> Result<RigProject> {
    if out_path.exists() {
        if let Ok(existing) = load_rig_project_file(out_path) {
            return Ok(existing);
        }
    }

    let legacy = YamlProjectRepository {
        path: legacy_path.to_path_buf(),
    }
    .load_current_project()
    .with_context(|| format!("failed to load legacy project {}", legacy_path.display()))?;

    let backup = PathBuf::from(format!("{}.bak", legacy_path.display()));
    if !backup.exists() {
        fs::copy(legacy_path, &backup)
            .with_context(|| format!("failed to back up {}", legacy_path.display()))?;
    }

    let rig = migrate_legacy_project(&legacy);
    rig.validate()
        .map_err(|e| anyhow!("migration produced invalid project: {e}"))?;
    save_rig_project_file(out_path, &rig)?;
    Ok(rig)
}

#[cfg(test)]
#[path = "rig_yaml_tests.rs"]
mod tests;
