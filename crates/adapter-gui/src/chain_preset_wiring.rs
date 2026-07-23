//! Wiring for chain preset save/load callbacks on the main window.
//!
//! Owns the 5 callbacks driving preset save/load and the touch-mode picker:
//!
//! - `on_save_chain_preset`     — touch-mode auto-saves to the presets dir,
//!   desktop opens a save dialog.
//! - `on_configure_chain_preset` — touch-mode shows the in-app picker (lists
//!   the presets dir), desktop opens a load dialog.
//! - `on_preset_picker_confirm` — touch picker → load + replace blocks.
//! - `on_preset_picker_cancel`  — closes the touch picker.
//! - `on_preset_picker_delete`  — touch picker → delete preset file.
//!
//! Both load paths preserve the chain's first Input and last Output blocks
//! (device config is per-machine, not per-preset) and assign fresh block ids.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::ChainId;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;
use project::rig::{humanize_preset_label, RigProject};

use crate::assign_new_block_ids;
use crate::helpers::{clear_status, set_status_error, set_status_info};
use crate::project_ops::{load_preset_file, sync_project_dirty};
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::sync_live_chain_runtime;
use crate::{AppWindow, ProjectChainItem};

pub(crate) struct ChainPresetCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    pub preset_file_list: Rc<RefCell<Vec<PathBuf>>>,
    pub auto_save: bool,
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainPresetCtx) {
    let ChainPresetCtx {
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
        preset_file_list,
        auto_save,
    } = ctx;

    // Issue #510: the in-window save overlay (single name + overwrite
    // confirm) is wired in a sibling module so this file stays under
    // the 600-line cap. Touch kiosk's direct auto-save still flows
    // through `on_save_chain_preset` registered there.
    crate::preset_save_wiring::wire(window, project_session.clone(), toast_timer.clone());

    // Issue #510: unfiltered (display_name, path) pairs backing the
    // load picker so the search field can re-filter without touching
    // disk on every keystroke. The visible `preset_file_list` and
    // `preset_picker_items` are always a filtered view of this.
    let preset_full_list: Rc<RefCell<Vec<(String, PathBuf)>>> = Rc::new(RefCell::new(Vec::new()));

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        let preset_file_list = preset_file_list.clone();
        let preset_full_list = preset_full_list.clone();
        window.on_configure_chain_preset(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-no-project-loaded"),
                );
                return;
            };
            // Always show the in-app preset picker (desktop + touch) so
            // the bundled presets are visible. Desktop previously used a
            // native FileDialog with no list — selection now flows
            // through on_preset_picker_confirm for both modes (#479).
            let mut full: Vec<(String, PathBuf)> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&session.presets_path) {
                let mut sorted: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map(|x| x == "yaml" || x == "yml")
                            .unwrap_or(false)
                    })
                    .collect();
                sorted.sort_by_key(|e| e.file_name());
                for entry in sorted {
                    let path = entry.path();
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .replace('_', " ");
                    full.push((name, path));
                }
            }
            *preset_full_list.borrow_mut() = full;
            // Issue #510: reset the search field every time the picker
            // opens so a stale query from a previous open doesn't hide
            // half the presets.
            window.set_preset_picker_search_query(SharedString::new());
            apply_preset_filter(&window, &preset_full_list, &preset_file_list, "");
            window.set_preset_picker_chain_index(index);
            window.set_show_preset_picker(true);
        });
    }
    {
        // Issue #510: re-filter the visible list every time the user
        // types in the search field. The full list stays on the
        // adapter side; we never re-read the directory mid-search.
        let weak_window = window.as_weak();
        let preset_full_list = preset_full_list.clone();
        let preset_file_list = preset_file_list.clone();
        window.on_preset_picker_query_changed(move |query| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            apply_preset_filter(
                &window,
                &preset_full_list,
                &preset_file_list,
                query.as_str(),
            );
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        let preset_file_list = preset_file_list.clone();
        window.on_preset_picker_confirm(move |preset_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_show_preset_picker(false);
            let files = preset_file_list.borrow();
            let Some(path) = files.get(preset_index as usize) else {
                return;
            };
            let path = path.clone();
            drop(files);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let chain_index = window.get_preset_picker_chain_index();
            match load_preset_file(&path) {
                Ok(preset) => {
                    // Hand the dispatcher I/O-stripped blocks (issue #518):
                    // it is the dispatcher's job to preserve the chain's
                    // existing Input/Output across the swap. Wrapping I/O
                    // here too would land two of each on the chain.
                    let preset_instrument = preset.instrument.clone();
                    let dispatch_result = {
                        let proj = session.project.borrow();
                        if let Some(chain) = proj.chains.get(chain_index as usize) {
                            let chain_id = chain.id.clone();
                            let stripped = strip_io_blocks(preset.blocks);
                            // Assign fresh IDs via a temporary chain struct.
                            let mut tmp_chain = Chain {
                                id: chain_id.clone(),
                                description: None,
                                instrument: String::new(),
                                enabled: false,
                                volume: 100.0,
                                io_binding_ids: vec![],
                                blocks: stripped,
                                di_output: None,
                                loopers: vec![],
                            };
                            assign_new_block_ids(&mut tmp_chain);
                            Some((chain_id, tmp_chain.blocks))
                        } else {
                            None
                        }
                    };
                    if let Some((chain_id, preset_blocks)) = dispatch_result {
                        if let Err(error) = session.dispatcher.dispatch(Command::LoadChainPreset {
                            chain: chain_id.clone(),
                            preset_instrument,
                            preset_blocks,
                        }) {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                        // Issue #510: round-trip contract — the active
                        // preset's display name follows the loaded file's
                        // stem verbatim so the combobox reflects exactly
                        // what the user picked.
                        if let Some(name) = preset_rename_target_from_path(&path) {
                            if let Err(e) = session.dispatcher.dispatch(Command::RenameRigPreset {
                                chain: chain_id.clone(),
                                name,
                            }) {
                                log::warn!("[preset] Command::RenameRigPreset falhou: {e}");
                            }
                        }
                        if let Err(error) =
                            sync_live_chain_runtime(&project_runtime, session, &chain_id)
                        {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                        replace_project_chains(
                            &project_chains,
                            &session.project.borrow(),
                            &input_chain_devices.borrow(),
                            &output_chain_devices.borrow(),
                            &[],
                        );
                        // Issue #510 bug fix: the chain preset combobox
                        // is fed by `chain-rig-nav`, not by `project_chains`.
                        // Without this refresh, `Command::RenameRigPreset`
                        // updates the rig in memory but the visible combo
                        // keeps the old label.
                        crate::chain_rig_nav_wiring::refresh_chain_rig_nav(&window, session);
                        sync_project_dirty(
                            &window,
                            session,
                            &saved_project_snapshot,
                            &project_dirty,
                            auto_save,
                        );
                        clear_status(&window, &toast_timer);
                    }
                }
                Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
            }
        });
    }
    {
        let weak_window = window.as_weak();
        window.on_preset_picker_cancel(move || {
            if let Some(window) = weak_window.upgrade() {
                window.set_show_preset_picker(false);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let preset_file_list = preset_file_list.clone();
        let preset_full_list = preset_full_list.clone();
        let toast_timer = toast_timer.clone();
        let project_session = project_session.clone();
        window.on_preset_picker_delete(move |preset_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(path) = preset_file_list
                .borrow()
                .get(preset_index as usize)
                .cloned()
            else {
                return;
            };
            // #555: deletion is business — `Command::DeleteChainPreset`
            // owns the `fs::remove_file` call inside the dispatcher
            // (`local_dispatcher_preset.rs`). The GUI only dispatches
            // the intent and refreshes its picker on success.
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let outcome = if let Some(session) = project_session.borrow().as_ref() {
                session
                    .dispatcher
                    .dispatch(Command::DeleteChainPreset { name })
            } else {
                Err(anyhow::anyhow!("no active session to dispatch on"))
            };
            match outcome {
                Ok(_) => {
                    // Issue #510: keep the full list (search source)
                    // in sync with disk; then re-apply the active
                    // query so the visible model stays consistent.
                    preset_full_list.borrow_mut().retain(|(_, p)| p != &path);
                    let query = window.get_preset_picker_search_query();
                    apply_preset_filter(
                        &window,
                        &preset_full_list,
                        &preset_file_list,
                        query.as_str(),
                    );
                    set_status_info(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("status-preset-removed"),
                    );
                }
                Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
            }
        });
    }
}

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
/// adapter can dispatch `Command::RenameRigPreset` after a successful
/// `Command::LoadChainPreset`. The name is the file's stem verbatim
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
fn apply_preset_filter(
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
    window.set_preset_picker_items(ModelRc::from(Rc::new(VecModel::from(visible_names))));
}

#[cfg(test)]
#[path = "chain_preset_wiring_tests.rs"]
mod tests;
