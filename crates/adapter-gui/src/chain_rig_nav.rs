//! Per-chain rig navigation rows (#436 #1) — pure projection used to
//! drive the preset/scene selectors on the legacy chains screen.
//!
//! The chains list is the synthetic legacy `Project` (one chain per rig
//! input, id `rig:<input>`). For each chain we expose the input's bank
//! (preset slots + names), the active preset's index, and the active
//! scene, in the SAME order as `project.chains` so the Slint row at
//! index `i` reads `rows[i]`. No Slint, no I/O — fully testable.

use project::project::Project;
use project::rig::RigProject;

/// One chain's rig preset/scene navigation state. Empty `preset_labels`
/// ⇒ not a rig chain (or input vanished) → the UI hides the selectors.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RigNavRow {
    /// Rig input name (chain id without the `rig:` prefix), or empty.
    pub(crate) input: String,
    /// Bank slot numbers, ascending (deterministic via `BTreeMap`).
    pub(crate) preset_slots: Vec<usize>,
    /// Preset display names, parallel to `preset_slots`.
    pub(crate) preset_labels: Vec<String>,
    /// Index into `preset_labels` of the active preset (0 if absent).
    pub(crate) active_index: usize,
    /// Active scene, `1..=8`.
    pub(crate) scene: usize,
}

/// Build the nav rows aligned 1:1 with `project.chains`.
pub(crate) fn rig_nav_rows(rig: &RigProject, project: &Project) -> Vec<RigNavRow> {
    project
        .chains
        .iter()
        .map(|chain| {
            let Some(name) = chain.id.0.strip_prefix("rig:") else {
                return RigNavRow::default();
            };
            let Some(input) = rig.inputs.get(name) else {
                return RigNavRow::default();
            };
            let preset_slots: Vec<usize> = input.bank.keys().copied().collect();
            let preset_labels: Vec<String> = input.bank.values().cloned().collect();
            let active_index = preset_slots
                .iter()
                .position(|&s| s == input.active_preset)
                .unwrap_or(0);
            RigNavRow {
                input: name.to_string(),
                preset_slots,
                preset_labels,
                active_index,
                scene: input.active_scene,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "chain_rig_nav_tests.rs"]
mod tests;
