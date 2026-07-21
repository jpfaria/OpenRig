//! `midi-map.yaml` — the legacy single-file controller mapping (#22) + the
//! resolved runtime view. The data types (`Source`, `Scale`, `Binding`) moved
//! to `project::midi` when the file split into a system **device profile** and
//! a project-owned **binding map** (ADR 0003 / #499). They are re-exported
//! here so existing code calling `adapter_midi::Binding` keeps working.
//!
//! Validation reuses `application::command_schema` (the single source of truth
//! for what a `Command` needs) so a renamed field or unknown command fails
//! the *load*, loudly, instead of silently dropping a binding.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

pub use project::midi::{Binding, Scale, Source};

/// The whole mapping file — the legacy `midi-map.yaml` shape. After the
/// system-vs-project split (ADR 0003 / #499) this struct represents the
/// **resolved** view at runtime: the device profile's `input` joined with
/// the project's `bindings` (or a system / shipped fallback).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MidiMap {
    /// Input device, matched by case-insensitive substring. `None` → use the
    /// system default input.
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

impl MidiMap {
    /// Load and **validate** the mapping. A binding whose command is unknown
    /// or whose args do not satisfy the `Command` schema is a hard error —
    /// the daemon must refuse to start rather than ignore bindings silently.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading MIDI map {}", path.display()))?;
        let map: MidiMap = serde_yaml::from_str(&raw)
            .with_context(|| format!("parsing MIDI map {}", path.display()))?;
        map.validate()?;
        Ok(map)
    }

    /// Probe every binding by building its `Command` (injecting a sample
    /// scaled value for continuous sources). Surfaces unknown commands and
    /// schema mismatches at load time.
    pub fn validate(&self) -> Result<()> {
        for (i, b) in self.bindings.iter().enumerate() {
            let mut args = b.args.clone();
            if let Some(scale) = &b.scale {
                inject(&mut args, &scale.into, scale.apply(64).into());
            }
            application::command_schema::command_from_variant(&b.command, args)
                .with_context(|| format!("binding #{i} ({})", b.command))?;
        }
        Ok(())
    }
}

/// Insert `value` at key `key` into a JSON object, creating the object if the
/// args were absent (`Null`).
pub(crate) fn inject(args: &mut Value, key: &str, value: Value) {
    if !args.is_object() {
        *args = Value::Object(serde_json::Map::new());
    }
    if let Some(obj) = args.as_object_mut() {
        obj.insert(key.to_string(), value);
    }
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
