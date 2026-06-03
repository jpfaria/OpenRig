//! One-shot migration of the legacy `midi-map.yaml` into the two #499 files:
//! a system [`crate::midi_profile`] (the `input:` field) and a system bindings
//! fallback (`bindings:` block). The legacy file is deleted after a successful
//! split so the migration is durable.
//!
//! The migration is intentionally **YAML-shape-only**: it never parses
//! `Binding` structs. That keeps `infra-filesystem` free of `project` and
//! `application` dependencies — validation happens later, at resolve time,
//! inside `adapter-midi` where it already lives. Migration only needs to
//! know that the legacy doc is a top-level mapping with optional `input:`
//! and optional `bindings:` keys.

use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::fs;
use std::path::Path;

/// Outcome of a migration attempt. Reported to callers so startup logs can
/// distinguish "nothing to do" from "migrated" from "left a broken file
/// alone".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// No legacy `midi-map.yaml` was found — nothing to do.
    NoLegacyFile,
    /// Legacy file existed and was split + deleted.
    Migrated,
    /// Legacy file existed but failed to parse as a YAML mapping. It was
    /// **left in place** so a human can inspect it; nothing was written.
    SkippedMalformed,
}

/// Split a legacy `midi-map.yaml` at `legacy_path` into its two #499 successor
/// files (`profile_path` for `input:`, `bindings_path` for `bindings:`) and
/// delete the legacy file. See [`MigrationOutcome`] for the reported result.
///
/// Idempotent: a missing legacy file is a no-op. Safe on a corrupted legacy
/// file: it stays in place so a human can repair it.
pub fn migrate_legacy_midi_map(
    legacy_path: &Path,
    profile_path: &Path,
    bindings_path: &Path,
) -> Result<MigrationOutcome> {
    if !legacy_path.exists() {
        return Ok(MigrationOutcome::NoLegacyFile);
    }

    let raw = fs::read_to_string(legacy_path)
        .with_context(|| format!("reading legacy MIDI map {}", legacy_path.display()))?;

    let parsed: Value = match serde_yaml::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            log::warn!(
                "legacy midi-map.yaml at {:?} is malformed, leaving in place: {error}",
                legacy_path
            );
            return Ok(MigrationOutcome::SkippedMalformed);
        }
    };

    // An empty/null doc is treated as "nothing to migrate" but still consumes
    // the legacy file so it doesn't get picked up next boot.
    let mapping = match parsed {
        Value::Mapping(m) => m,
        Value::Null => Mapping::new(),
        _ => {
            log::warn!(
                "legacy midi-map.yaml at {:?} is not a YAML mapping, leaving in place",
                legacy_path
            );
            return Ok(MigrationOutcome::SkippedMalformed);
        }
    };

    let input = mapping.get(Value::String("input".into())).cloned();
    let bindings = mapping.get(Value::String("bindings".into())).cloned();

    if let Some(input_value) = input {
        let mut profile_doc = Mapping::new();
        profile_doc.insert(Value::String("input".into()), input_value);
        write_yaml_doc(profile_path, &Value::Mapping(profile_doc))
            .with_context(|| format!("writing MIDI profile {}", profile_path.display()))?;
    }

    if let Some(bindings_value) = bindings {
        // Empty list is still meaningful (the user explicitly cleared the
        // map) — persist it; absent bindings yield no file at all.
        let mut bindings_doc = Mapping::new();
        bindings_doc.insert(Value::String("bindings".into()), bindings_value);
        write_yaml_doc(bindings_path, &Value::Mapping(bindings_doc))
            .with_context(|| format!("writing MIDI bindings {}", bindings_path.display()))?;
    }

    fs::remove_file(legacy_path)
        .with_context(|| format!("removing legacy MIDI map {}", legacy_path.display()))?;

    Ok(MigrationOutcome::Migrated)
}

fn write_yaml_doc(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating dir {}", parent.display()))?;
    }
    let raw = serde_yaml::to_string(value)?;
    fs::write(path, raw)?;
    Ok(())
}
