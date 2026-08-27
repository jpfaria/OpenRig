//! Responsibility: answers what presets a chain's bank holds.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{Global, ModelRc, SharedString, VecModel};

use domain::ids::ChainId;
use project::block::{AudioBlock, AudioBlockKind};
use project::rig::{humanize_preset_label, RigProject};

use crate::AppWindow;

/// Drop Input/Output blocks from a preset's block list before it is
/// dispatched onto a chain. The dispatcher owns the chain's I/O
/// across a preset swap (it preserves the existing endpoints), so the
/// adapter MUST hand it I/O-stripped blocks — otherwise both layers
/// wrap I/O and the chain ends up with duplicates. Issue #518.
pub(crate) fn strip_io_blocks(blocks: Vec<AudioBlock>) -> Vec<AudioBlock> {
    blocks
        .into_iter()
        .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
        .collect()
}

/// Slug the active preset's name into a filesystem-safe stem for the
/// save dialog / kiosk auto-save. The chain title moved to
/// `input.label` after #436, so reusing `chain.description` for the
/// filename now reflects the chain, not the preset. Issue #518.
///
/// Returns `None` for chains that are not projected from a rig input
/// (i.e. no `rig:` prefix, or the input/preset is missing) — the
/// caller decides the fallback (typically the chain's own slug).
/// #323 phase 2: the id (bank key) of the preset a rig chain is currently
/// playing — what RECORD links a fresh loop to, so the loop keeps that tone
/// even after the chain switches preset to solo. `None` for a non-rig chain or
/// a missing input.
pub(crate) fn active_preset_id(chain_id: &ChainId, rig: &RigProject) -> Option<String> {
    let input_name = chain_id.0.strip_prefix("rig:")?;
    let input = rig.inputs.get(input_name)?;
    input.bank.get(&input.active_preset).cloned()
}

/// #323 phase 2: a rig chain's bank as `(id, display-name)` pairs, in bank-slot
/// order — the source for the looper preset picker's options and its id map.
/// Empty for a non-rig chain.
pub(crate) fn chain_preset_bank(chain_id: &ChainId, rig: &RigProject) -> Vec<(String, String)> {
    let Some(input_name) = chain_id.0.strip_prefix("rig:") else {
        return Vec::new();
    };
    let Some(input) = rig.inputs.get(input_name) else {
        return Vec::new();
    };
    // BTreeMap ⇒ ascending slot order, matching the preset combobox.
    input
        .bank
        .values()
        .map(|id| {
            let label = rig
                .presets
                .get(id)
                .and_then(|p| p.name.clone())
                .unwrap_or_else(|| humanize_preset_label(id));
            (id.clone(), label)
        })
        .collect()
}

pub(crate) fn default_preset_filename_slug(chain_id: &ChainId, rig: &RigProject) -> Option<String> {
    let input_name = chain_id.0.strip_prefix("rig:")?;
    let input = rig.inputs.get(input_name)?;
    let preset_key = input.bank.get(&input.active_preset)?;
    let preset = rig.presets.get(preset_key)?;
    // Issue #510 user feedback: return the preset display name
    // verbatim. The function name is kept for git history; semantics
    // changed from "slug form" to "user-visible name as-is".
    Some(
        preset
            .name
            .clone()
            .unwrap_or_else(|| humanize_preset_label(preset_key)),
    )
}

// `PRESET_EXTENSION`, `sanitize_for_filename`, `preset_filename` and
// `preset_save_path` moved to `application::preset_file` in issue
// #555 so the dispatcher can resolve preset paths without
// re-implementing the helpers. Re-exported for the existing
// in-crate callers (`preset_save_wiring`, `chain_preset_wiring_tests`).
#[allow(unused_imports)] // `preset_filename` is only consumed from tests.
pub(crate) use application::preset_file::{preset_filename, preset_save_path};

/// Derive the preset display name from a loaded file path so the
/// adapter can dispatch `SelectionCommand::RenameRigPreset` after a successful
/// `ChainCommand::LoadChainPreset`. The name is the file's stem verbatim
/// — no humanization. Earlier versions ran `humanize_preset_label`
/// here and silently rewrote dashes/underscores, surprising users who
/// chose those characters deliberately. Issue #510 round-trip
/// contract: the preset's name follows the file the user picked.
pub(crate) fn preset_rename_target_from_path(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    if stem.is_empty() {
        return None;
    }
    Some(stem.to_string())
}

/// Case-insensitive substring filter for the load picker's search
/// field. Empty query passes everything through. Issue #510.
pub(crate) fn filter_preset_names<'a>(names: &'a [String], query: &str) -> Vec<&'a String> {
    // Share the matching predicate with the bank dropdown's search
    // (`preset_search`) so both fields behave identically. Issue #659.
    let q = query.trim().to_lowercase();
    names
        .iter()
        .filter(|n| crate::preset_search::preset_label_matches(n, &q))
        .collect()
}

/// Returns `true` when saving a preset under `name` would overwrite an
/// existing file in `presets_dir`. Issue #510 — drives the in-window
/// overwrite confirmation modal.
pub(crate) fn preset_overwrite_required(presets_dir: &std::path::Path, name: &str) -> bool {
    preset_save_path(presets_dir, name).exists()
}

/// Apply the current search query to the load picker's full list and
/// publish the filtered view onto the AppWindow (items + file list).
/// Centralized so `on_configure_chain_preset`, the query-changed
/// callback and `on_preset_picker_delete` all stay in sync. Issue #510.
pub(crate) fn apply_preset_filter(
    window: &AppWindow,
    full: &Rc<RefCell<Vec<(String, PathBuf)>>>,
    visible: &Rc<RefCell<Vec<PathBuf>>>,
    query: &str,
) {
    let full = full.borrow();
    let all_names: Vec<String> = full.iter().map(|(n, _)| n.clone()).collect();
    let kept = filter_preset_names(&all_names, query);
    let kept_set: std::collections::HashSet<&String> = kept.into_iter().collect();
    let mut visible_paths: Vec<PathBuf> = Vec::with_capacity(full.len());
    let mut visible_names: Vec<SharedString> = Vec::with_capacity(full.len());
    for (name, path) in full.iter() {
        if kept_set.contains(name) {
            visible_paths.push(path.clone());
            visible_names.push(name.clone().into());
        }
    }
    *visible.borrow_mut() = visible_paths;
    crate::OverlayBridge::get(window)
        .set_preset_picker_items(ModelRc::from(Rc::new(VecModel::from(visible_names))));
}

#[cfg(test)]
#[path = "chain_preset_bank_tests.rs"]
mod bank_tests;
