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
mod tests {
    use super::*;

    fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("openrig-midimap-{name}.yaml"));
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn parses_all_source_kinds_and_optional_input() {
        let p = write_tmp(
            "kinds",
            r#"
input: Chocolate
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: "chain:a", block: "block:b" }
  - source: { kind: program_change, program: 5 }
    command: SaveProject
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "chain:a", block: "block:b", path: gain }
    scale: { min: 0.0, max: 100.0 }
"#,
        );
        let map = MidiMap::load(&p).unwrap();
        assert_eq!(map.input.as_deref(), Some("Chocolate"));
        assert_eq!(map.bindings.len(), 3);
        assert_eq!(
            map.bindings[0].source,
            Source::NoteOn {
                channel: 1,
                note: 60
            }
        );
        assert!(map.bindings[2].source.is_continuous());
        assert_eq!(map.bindings[2].scale.as_ref().unwrap().into, "value");
    }

    #[test]
    fn default_input_is_none() {
        let p = write_tmp("noinput", "bindings: []\n");
        let map = MidiMap::load(&p).unwrap();
        assert!(map.input.is_none());
        assert!(map.bindings.is_empty());
    }

    #[test]
    fn load_rejects_unknown_command() {
        let p = write_tmp(
            "unknown",
            r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: NotARealCommand
"#,
        );
        let err = MidiMap::load(&p).unwrap_err().to_string();
        assert!(err.contains("binding #0"), "{err}");
    }

    #[test]
    fn load_rejects_args_violating_command_schema() {
        // ToggleBlockEnabled needs string ids; a number fails the schema.
        let p = write_tmp(
            "badargs",
            r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: 0, block: 1 }
"#,
        );
        let err = MidiMap::load(&p).unwrap_err().to_string();
        assert!(err.contains("binding #0"), "{err}");
    }

    #[test]
    fn scale_apply_is_linear_full_range() {
        let s = Scale {
            min: 0.0,
            max: 100.0,
            into: "value".into(),
        };
        assert!((s.apply(0) - 0.0).abs() < 1e-9);
        assert!((s.apply(127) - 100.0).abs() < 1e-9);
        assert!((s.apply(64) - 50.39).abs() < 0.5);
    }

    #[test]
    fn scaled_continuous_binding_validates_with_probe_value() {
        // Args omit `value`; validation must inject the scaled probe so the
        // schema check passes (the value arrives at runtime from the pedal).
        let p = write_tmp(
            "scaled",
            r#"
bindings:
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "chain:a", block: "block:b", path: gain }
    scale: { min: 0.0, max: 100.0 }
"#,
        );
        assert!(MidiMap::load(&p).is_ok());
    }
}
