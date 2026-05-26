//! MIDI profile schema (issue #548).
//!
//! A profile is a declarative YAML file shipped per controller model:
//! a list of `{ when: <MIDI message>, do: <slot name> }` bindings. The
//! 20 slot names are fixed at compile time — `do:` referencing an
//! unknown slot is rejected at parse time so a malformed asset cannot
//! reach the runtime.
//!
//! See `docs/superpowers/specs/2026-05-26-midi-profiles-design.md`.

use serde::Deserialize;

/// The 20 MIDI-controllable actions of OpenRig (V1, frozen). Order is
/// stable and mirrors `docs/superpowers/specs/2026-05-26-midi-profiles-design.md`.
pub const CATALOG: &[&str] = &[
    // App (1-3)
    "toggle_tuner",
    "toggle_output_mute",
    "toggle_spectrum",
    // Chain — active (4-13)
    "prev_chain",
    "next_chain",
    "toggle_active_chain_enabled",
    "toggle_compact_view",
    "prev_preset",
    "next_preset",
    "prev_scene",
    "next_scene",
    "jump_preset_n",
    "jump_scene_n",
    // Blocks — active (14-18)
    "prev_block_1",
    "next_block_1",
    "prev_block_2",
    "next_block_2",
    "toggle_active_block_enabled",
    // Continuous — CC (19-20)
    "chain_volume",
    "block_param_numeric",
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MidiProfile {
    pub name: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Binding {
    pub when: MatchExpr,
    #[serde(rename = "do")]
    pub action: String,
}

/// A MIDI message pattern. Tagged by `kind` (using the MIDI 1.0 spec names).
/// The per-kind value field (`note` / `controller` / `program`) is optional —
/// absence means "wildcard, match any value" (used by `jump_preset_n` and
/// `jump_scene_n` where the message value becomes the action parameter).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind")]
pub enum MatchExpr {
    NoteOn {
        channel: u8,
        #[serde(default)]
        note: Option<u8>,
    },
    NoteOff {
        channel: u8,
        #[serde(default)]
        note: Option<u8>,
    },
    ControlChange {
        channel: u8,
        #[serde(default)]
        controller: Option<u8>,
    },
    ProgramChange {
        channel: u8,
        #[serde(default)]
        program: Option<u8>,
    },
}

#[derive(Debug)]
pub enum ProfileError {
    Yaml(serde_yaml::Error),
    UnknownSlot(String),
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::Yaml(e) => write!(f, "yaml parse error: {e}"),
            ProfileError::UnknownSlot(name) => write!(
                f,
                "unknown MIDI slot '{name}' — not in the 20-slot catalog"
            ),
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<serde_yaml::Error> for ProfileError {
    fn from(e: serde_yaml::Error) -> Self {
        ProfileError::Yaml(e)
    }
}

pub fn parse_profile_yaml(yaml: &str) -> Result<MidiProfile, ProfileError> {
    let profile: MidiProfile = serde_yaml::from_str(yaml)?;
    for binding in &profile.bindings {
        if !CATALOG.contains(&binding.action.as_str()) {
            return Err(ProfileError::UnknownSlot(binding.action.clone()));
        }
    }
    Ok(profile)
}

/// Scan a directory for `*.yaml` files and parse each one. Malformed
/// or out-of-spec files are skipped (logged) rather than panicking, so
/// a single bad user profile doesn't take MIDI down. Missing directory
/// is also OK — returns empty.
pub fn load_profiles_from_dir(dir: &std::path::Path) -> Vec<MidiProfile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(yaml) => match parse_profile_yaml(&yaml) {
                Ok(p) => profiles.push(p),
                Err(e) => log::warn!("skipping {}: {e}", path.display()),
            },
            Err(e) => log::warn!("can't read {}: {e}", path.display()),
        }
    }
    profiles
}
