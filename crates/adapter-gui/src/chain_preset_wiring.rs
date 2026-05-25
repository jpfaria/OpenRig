//! Wiring for chain preset save/load callbacks on the main window.
//!
//! Owns the 5 callbacks driving preset save/load and the touch-mode picker:
//!
//! - `on_save_chain_preset`     — touch-mode auto-saves to the presets dir,
//!                                desktop opens a save dialog.
//! - `on_configure_chain_preset` — touch-mode shows the in-app picker (lists
//!                                 the presets dir), desktop opens a load dialog.
//! - `on_preset_picker_confirm` — touch picker → load + replace blocks.
//! - `on_preset_picker_cancel`  — closes the touch picker.
//! - `on_preset_picker_delete`  — touch picker → delete preset file.
//!
//! Both load paths preserve the chain's first Input and last Output blocks
//! (device config is per-machine, not per-preset) and assign fresh block ids.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rfd::FileDialog;
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
use crate::project_ops::{load_preset_file, save_chain_blocks_to_preset, sync_project_dirty};
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

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_chain_preset(move |index| {
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
            let (chain_desc, chain_clone, chain_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    drop(proj);
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                };
                (
                    chain
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("chain_{}", index + 1)),
                    chain.clone(),
                    chain.id.clone(),
                )
            };
            // Issue #518: filename = active preset's name (slug), not
            // the chain's title (which is `input.label` after #436).
            // Fall back to the chain's own slug for non-rig chains or
            // when the rig is unavailable.
            let preset_slug = session
                .rig
                .as_ref()
                .and_then(|r| default_preset_filename_slug(&chain_id, &r.borrow()));
            let default_name =
                preset_slug.unwrap_or_else(|| chain_desc.replace(' ', "_").to_lowercase());
            let path = if window.get_touch_optimized() {
                // Kiosk: auto-save to presets dir, no dialog
                let _ = std::fs::create_dir_all(&session.presets_path);
                preset_save_path(&session.presets_path, &default_name)
            } else {
                // Desktop: use file dialog
                let Some(p) = FileDialog::new()
                    .add_filter("OpenRig Preset", &["yaml", "yml"])
                    .set_title(rust_i18n::t!("dialog-save-preset").as_ref())
                    .set_directory(&session.presets_path)
                    .set_file_name(preset_filename(&default_name))
                    .save_file()
                else {
                    return;
                };
                p
            };
            match save_chain_blocks_to_preset(&chain_clone, &path) {
                Ok(()) => {
                    // #436 F: salvar preset é negócio → Command no
                    // dispatcher compartilhado (MCP/MIDI, observável via
                    // Event::ChainPresetSaved). O write do arquivo acima
                    // é adapter-side (precedente SaveProject).
                    if let Err(e) = session.dispatcher.dispatch(Command::SaveChainPreset {
                        name: default_name.clone(),
                    }) {
                        log::warn!("[preset] Command::SaveChainPreset falhou: {e}");
                    }
                    set_status_info(&window, &toast_timer, &rust_i18n::t!("status-preset-saved"))
                }
                Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        let preset_file_list = preset_file_list.clone();
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
            let mut files: Vec<PathBuf> = Vec::new();
            let mut names: Vec<SharedString> = Vec::new();
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
                    names.push(name.into());
                    files.push(path);
                }
            }
            *preset_file_list.borrow_mut() = files;
            window.set_preset_picker_items(ModelRc::from(Rc::new(VecModel::from(names))));
            window.set_preset_picker_chain_index(index);
            window.set_show_preset_picker(true);
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
                                blocks: stripped,
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
                            preset_blocks,
                        }) {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                        // Issue #510: round-trip contract — the active
                        // preset's display name follows the loaded file's
                        // stem (humanized) so the combobox immediately
                        // reflects what the user picked.
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
                            &*session.project.borrow(),
                            &input_chain_devices.borrow(),
                            &output_chain_devices.borrow(),
                        );
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
        let toast_timer = toast_timer.clone();
        let project_session = project_session.clone();
        window.on_preset_picker_delete(move |preset_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut files = preset_file_list.borrow_mut();
            let Some(path) = files.get(preset_index as usize).cloned() else {
                return;
            };
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    // #436 F: apagar preset é negócio → Command no
                    // dispatcher compartilhado (MCP/MIDI, observável via
                    // Event::ChainPresetDeleted). O remove_file acima é
                    // adapter-side (precedente SaveProject).
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(session) = project_session.borrow().as_ref() {
                        if let Err(e) = session
                            .dispatcher
                            .dispatch(Command::DeleteChainPreset { name })
                        {
                            log::warn!("[preset] Command::DeleteChainPreset falhou: {e}");
                        }
                    }
                    files.remove(preset_index as usize);
                    let names: Vec<SharedString> = files
                        .iter()
                        .map(|p| {
                            p.file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .replace('_', " ")
                                .into()
                        })
                        .collect();
                    window.set_preset_picker_items(ModelRc::from(Rc::new(VecModel::from(names))));
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
    let display = preset
        .name
        .clone()
        .unwrap_or_else(|| humanize_preset_label(preset_key));
    Some(display.replace(' ', "_").to_lowercase())
}

/// Suffix used for preset files on disk. Issue #510 centralizes this
/// so `preset_filename`, `preset_save_path` and the load filter stay
/// in sync. The picker still accepts `.yml` for legacy bundles.
const PRESET_EXTENSION: &str = "yaml";

/// Slug a preset name into the on-disk filename (without the directory).
/// Issue #510: the save dialog only asks for a name; the adapter alone
/// decides the file's extension and lowercasing convention.
pub(crate) fn preset_filename(name: &str) -> String {
    let slug = name.trim().replace(' ', "_").to_lowercase();
    format!("{slug}.{PRESET_EXTENSION}")
}

/// Resolve the absolute save path for a preset under the configured
/// presets directory. Issue #510.
pub(crate) fn preset_save_path(presets_dir: &std::path::Path, name: &str) -> PathBuf {
    presets_dir.join(preset_filename(name))
}

/// Derive the preset display name from a loaded file path so the
/// adapter can dispatch `Command::RenameRigPreset` after a successful
/// `Command::LoadChainPreset`. The file's stem is the slug convention
/// (`silverchair_freak`); the display name is the humanized form
/// (`Silverchair Freak`). Issue #510 round-trip contract.
pub(crate) fn preset_rename_target_from_path(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    if stem.is_empty() {
        return None;
    }
    Some(humanize_preset_label(&stem.replace('_', "-")))
}

#[cfg(test)]
#[path = "chain_preset_wiring_tests.rs"]
mod tests;
