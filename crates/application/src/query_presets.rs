//! Responsibility: lists the presets a rig holds.

use domain::ids::ChainId;
use project::rig::RigProject;
use std::fmt::Write;

use crate::json_fmt::json_string;

/// #554: return the bank of chain-scoped presets for one chain as JSON.
///
/// The `chain_id` must be of the form `rig:<input-name>`. The input's
/// `bank: BTreeMap<usize, String>` (slot → preset name) is emitted as a
/// `slots` array sorted by slot index, plus the resolved `active_preset`
/// name (or `null` when the bank is empty / the active slot is unbound).
///
/// Reads from the in-memory `RigProject` only — never the filesystem.
/// The disk-side preset library (under `config.paths.presets_path`) is a
/// separate concept tracked by a different follow-up.
pub fn list_chain_presets(rig: &RigProject, chain_id: &ChainId) -> Result<String, String> {
    let input_name = chain_id.0.strip_prefix("rig:").ok_or_else(|| {
        format!(
            "chain id '{}' is not a rig: input (expected 'rig:<input-name>')",
            chain_id.0
        )
    })?;
    let input = rig.inputs.get(input_name).ok_or_else(|| {
        format!(
            "input '{input_name}' not found in project (chain id '{}' references no live input)",
            chain_id.0
        )
    })?;

    let mut slots = String::new();
    slots.push('[');
    let mut first = true;
    for (idx, preset_key) in input.bank.iter() {
        if !first {
            slots.push(',');
        }
        first = false;
        let label = rig
            .presets
            .get(preset_key)
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| preset_key.clone());
        let _ = write!(
            slots,
            "{{\"index\":{},\"name\":{},\"key\":{}}}",
            idx,
            json_string(&label),
            json_string(preset_key)
        );
    }
    slots.push(']');

    let active_preset = input
        .bank
        .get(&input.active_preset)
        .map(|key| {
            let label = rig
                .presets
                .get(key)
                .and_then(|p| p.name.clone())
                .unwrap_or_else(|| key.clone());
            json_string(&label)
        })
        .unwrap_or_else(|| "null".to_string());

    Ok(format!(
        "{{\"chain\":{},\"active_preset\":{},\"slots\":{}}}",
        json_string(&chain_id.0),
        active_preset,
        slots
    ))
}

/// #554 follow-up: list every named preset in `RigProject.presets` —
/// the in-memory pool that the rig's input banks reference by key.
/// A preset can sit in the pool without being bound to any input slot
/// yet (e.g. the user saved it via the rig screen but hasn't wired it
/// into a chain). The tone-builder skill's Step 0 reads this to make
/// sure it doesn't silently overwrite an existing preset on save.
///
/// Each entry returns the user-visible label (`RigPreset.name`,
/// falling back to the pool key when the field is absent) AND the
/// pool key — the key is what the bank slots reference and what
/// `Command::DeleteChainPreset` / load operations take, so consumers
/// need both. Sorted by display label so the GUI's combobox and the
/// MCP read produce the same order.
///
/// Pure: `&RigProject` in, `String` out. Reads the in-memory rig
/// only — the on-disk preset library (`config.paths.presets_path`)
/// is a separate concern.
pub fn list_project_presets(rig: &RigProject) -> String {
    let mut entries: Vec<(&String, String)> = rig
        .presets
        .iter()
        .map(|(key, preset)| {
            let label = preset.name.clone().unwrap_or_else(|| key.clone());
            (key, label)
        })
        .collect();
    entries.sort_by(|(_, a), (_, b)| a.cmp(b));
    let mut out = String::from("{\"presets\":[");
    let mut first = true;
    for (key, label) in entries {
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(
            out,
            "{{\"name\":{},\"key\":{}}}",
            json_string(&label),
            json_string(key)
        );
    }
    out.push_str("]}");
    out
}
