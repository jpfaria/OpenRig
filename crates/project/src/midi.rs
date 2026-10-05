//! Responsibility: describes the MIDI bindings a project carries.
//! Project-level MIDI binding data types — owned by [`crate::rig::RigProject`]
//! so they travel with the `project.yaml` file (ADR 0003 / #499).
//!
//! Bindings used to live in `adapter-midi` (issue #22). They moved here when
//! the `midi-map.yaml` single-file model split into a system **device profile**
//! (which controller, system-level) and a project **binding map** (what each
//! binding does, project-level). The runtime validator that probes each
//! binding against the `Command` schema stays in `adapter-midi`; this module
//! is pure data so the `project` crate stays free of `application`/runtime
//! dependencies.
//!
//! Wire format is preserved bit-for-bit so existing `midi-map.yaml` files —
//! and the shipped `examples/midi-map.default.yaml` — keep parsing
//! unchanged.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One controller event to bind against. Internally tagged on `kind` in
/// YAML: `source: { kind: note_on, channel: 1, note: 60 }`. `channel` is
/// 1..=16 (human numbering, not the 0..=15 wire value).
///
/// `JsonSchema` is derived so this type can appear inside `Command` /
/// `Event` variants used by the MCP/gRPC schema surface (#513).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    NoteOn {
        channel: u8,
        note: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    Cc {
        channel: u8,
        controller: u8,
    },
    ProgramChange {
        program: u8,
    },
    /// A 14-bit fader — what Mackie Control surfaces (e.g. SMC-Mixer) send
    /// per channel strip (#1007).
    PitchBend {
        channel: u8,
    },
}

impl Source {
    /// Continuous sources carry a value (0..=127 CC, 0..=16383 Pitch Bend)
    /// that gets scaled into a `Command` argument; discrete sources fire a
    /// fixed command.
    pub fn is_continuous(&self) -> bool {
        matches!(self, Source::Cc { .. } | Source::PitchBend { .. })
    }
}

fn default_into() -> String {
    "value".to_string()
}

/// Linear map of a continuous source's 0..=127 value into `[min, max]`,
/// written into the command argument named `into` (default `value`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Scale {
    pub min: f64,
    pub max: f64,
    #[serde(default = "default_into")]
    pub into: String,
}

impl Scale {
    /// Map a raw 0..=127 MIDI value linearly into `[min, max]`.
    pub fn apply(&self, raw: u8) -> f64 {
        let t = f64::from(raw) / 127.0;
        self.min + t * (self.max - self.min)
    }

    /// Map a normalised `0..=1` position (e.g. a 14-bit Pitch Bend fader)
    /// linearly into `[min, max]`.
    pub fn apply_unit(&self, t: f64) -> f64 {
        self.min + t * (self.max - self.min)
    }

    /// Inverse of [`Scale::apply_unit`], clamped to `0..=1` — where a motor
    /// fader has to sit to show `value`.
    pub fn unit_of(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span == 0.0 {
            return 0.0;
        }
        ((value - self.min) / span).clamp(0.0, 1.0)
    }
}

/// One binding: a source, the `Command` variant name (PascalCase, as in the
/// `Command` enum), its static JSON args, and an optional scale for
/// continuous sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Binding {
    pub source: Source,
    pub command: String,
    #[serde(default, skip_serializing_if = "value_is_null")]
    pub args: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
}

fn value_is_null(v: &Value) -> bool {
    v.is_null()
}

/// Project-level MIDI configuration — the `midi:` block inside
/// `project.yaml`. Holds only the bindings (project layer); the device
/// profile (which controller to listen to) lives system-side per ADR 0003.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, JsonSchema)]
pub struct RigProjectMidi {
    /// Bindings active for this rig. When the project carries `midi:`, these
    /// override the system-side fallback (`midi-bindings.yaml`) and the
    /// shipped default map in one step — see the resolver in `adapter-midi`.
    #[serde(default)]
    pub bindings: Vec<Binding>,
}
