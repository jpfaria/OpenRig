//! `RigCommand` — the per-input rig nav actions (preset/scene
//! switch/add/remove) as an explicit, pure command. The GUI maps a
//! Slint click to a `RigCommand` and applies it; tests dispatch the
//! same command and validate the business — no rendering, no manual QA.
//!
//! Split (#436): the Slint callback emits an `i32` sentinel; the pure
//! `rig_command_from_*` mappers turn it into a `RigCommand`, so both
//! "the click sends the right command" and "the command does the right
//! thing" are unit-tested.

use crate::rig::RigProject;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigCommand {
    /// Activate the preset at ComboBox `position` (positional index into
    /// the bank, ascending by key — translated to the real bank key).
    SwitchPreset { input: String, position: usize },
    /// Add a preset to the input's bank (fresh; becomes active).
    AddPreset { input: String },
    /// Remove the active preset (can't remove the last; reactivates another).
    RemovePreset { input: String },
    /// Activate scene `scene` (`1..=8`).
    SwitchScene { input: String, scene: usize },
    /// Append the next scene (snapshot of the active; becomes active).
    AddScene { input: String },
    /// Pop the last scene (can't remove the only remaining one).
    RemoveScene { input: String },
}

impl RigCommand {
    /// Apply to `rig`. `Some(())` on success; `None` when invalid
    /// (unknown input, position/scene out of range, only-scene removal)
    /// — the caller ignores a `None` instead of corrupting state.
    pub fn apply(&self, rig: &mut RigProject) -> Option<()> {
        match self {
            RigCommand::SwitchPreset { input, position } => {
                let key = rig.inputs.get(input)?.bank.keys().nth(*position).copied()?;
                let name = rig.inputs.get(input)?.bank.get(&key)?.clone();
                rig.presets.get(&name)?; // reject a dangling bank entry
                let ri = rig.inputs.get_mut(input)?;
                ri.active_preset = key;
                // #535: scenes belong to the preset. Carrying the previous
                // preset's active_scene leaks into the new preset on the
                // next write_back_processing_blocks call (the save path
                // runs CaptureRigEdits, which materializes a phantom scene
                // via `scenes.entry(idx).or_default()`). Reset to 1 — same
                // contract `add_preset_to_input` already enforces.
                ri.active_scene = 1;
                Some(())
            }
            RigCommand::AddPreset { input } => rig.add_preset_to_input(input).map(|_| ()),
            RigCommand::RemovePreset { input } => rig.remove_preset_from_input(input).map(|_| ()),
            RigCommand::SwitchScene { input, scene } => {
                if !(1..=8).contains(scene) {
                    return None;
                }
                rig.inputs.get_mut(input)?.active_scene = *scene;
                Some(())
            }
            RigCommand::AddScene { input } => rig.add_scene_to_input(input).map(|_| ()),
            RigCommand::RemoveScene { input } => {
                rig.remove_last_scene_from_input(input).map(|_| ())
            }
        }
    }
}

/// The preset select emits an `i32`: `>= 0` is the ComboBox position;
/// the sentinel `-1` means "add a preset". Pure — unit-tested so the
/// click→command binding can't silently drift.
pub fn rig_command_from_select(input: &str, slot: i32) -> RigCommand {
    match slot {
        -1 => RigCommand::AddPreset {
            input: input.to_string(),
        },
        -2 => RigCommand::RemovePreset {
            input: input.to_string(),
        },
        n => RigCommand::SwitchPreset {
            input: input.to_string(),
            position: n.max(0) as usize,
        },
    }
}

/// The scene bar emits an `i32`: `>= 1` selects that scene; `-1` adds
/// the next scene; `-2` removes the last. Pure — unit-tested.
pub fn rig_command_from_scene(input: &str, scene: i32) -> RigCommand {
    match scene {
        -1 => RigCommand::AddScene {
            input: input.to_string(),
        },
        -2 => RigCommand::RemoveScene {
            input: input.to_string(),
        },
        n => RigCommand::SwitchScene {
            input: input.to_string(),
            scene: n.max(0) as usize,
        },
    }
}

#[cfg(test)]
#[path = "rig_command_tests.rs"]
mod rig_command_tests;
