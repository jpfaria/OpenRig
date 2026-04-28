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

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::{AudioBlock, AudioBlockKind};

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
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            let default_name = chain
                .description
                .clone()
                .unwrap_or_else(|| format!("chain_{}", index + 1))
                .replace(' ', "_")
                .to_lowercase();
            let path = if window.get_touch_optimized() {
                // Kiosk: auto-save to presets dir, no dialog
                let _ = std::fs::create_dir_all(&session.presets_path);
                session.presets_path.join(format!("{default_name}.yaml"))
            } else {
                // Desktop: use file dialog
                let Some(p) = FileDialog::new()
                    .add_filter("OpenRig Preset", &["yaml", "yml"])
                    .set_title("Salvar preset")
                    .set_directory(&session.presets_path)
                    .set_file_name(format!("{default_name}.yaml"))
                    .save_file()
                else {
                    return;
                };
                p
            };
            match save_chain_blocks_to_preset(chain, &path) {
                Ok(()) => set_status_info(&window, &toast_timer, "Preset salvo."),
                Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
            }
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
        window.on_configure_chain_preset(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            if window.get_touch_optimized() {
                // Kiosk: scan presets dir and show in-app picker
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
                return;
            }
            // Desktop: use file dialog
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Preset", &["yaml", "yml"])
                .set_title("Carregar preset na chain")
                .set_directory(&session.presets_path)
                .pick_file()
            else {
                return;
            };
            match load_preset_file(&path) {
                Ok(preset) => {
                    if let Some(chain) = session.project.chains.get_mut(index as usize) {
                        // Preserve I/O blocks (device config is per-machine, not per-preset).
                        // Keep the first Input and last Output; replace everything between.
                        let first_input = chain
                            .blocks
                            .iter()
                            .find(|b| matches!(b.kind, AudioBlockKind::Input(_)))
                            .cloned();
                        let last_output = chain
                            .blocks
                            .iter()
                            .rev()
                            .find(|b| matches!(b.kind, AudioBlockKind::Output(_)))
                            .cloned();
                        let mut new_blocks: Vec<AudioBlock> = Vec::new();
                        if let Some(input) = first_input {
                            new_blocks.push(input);
                        }
                        new_blocks.extend(preset.blocks.into_iter().filter(|b| {
                            !matches!(
                                b.kind,
                                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
                            )
                        }));
                        if let Some(output) = last_output {
                            new_blocks.push(output);
                        }
                        chain.blocks = new_blocks;
                        assign_new_block_ids(chain);
                        let chain_id = chain.id.clone();
                        if let Err(error) =
                            sync_live_chain_runtime(&project_runtime, session, &chain_id)
                        {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                        replace_project_chains(
                            &project_chains,
                            &session.project,
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
                    if let Some(chain) = session.project.chains.get_mut(chain_index as usize) {
                        let first_input = chain
                            .blocks
                            .iter()
                            .find(|b| matches!(b.kind, AudioBlockKind::Input(_)))
                            .cloned();
                        let last_output = chain
                            .blocks
                            .iter()
                            .rev()
                            .find(|b| matches!(b.kind, AudioBlockKind::Output(_)))
                            .cloned();
                        let mut new_blocks: Vec<AudioBlock> = Vec::new();
                        if let Some(input) = first_input {
                            new_blocks.push(input);
                        }
                        new_blocks.extend(preset.blocks.into_iter().filter(|b| {
                            !matches!(
                                b.kind,
                                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
                            )
                        }));
                        if let Some(output) = last_output {
                            new_blocks.push(output);
                        }
                        chain.blocks = new_blocks;
                        assign_new_block_ids(chain);
                        let chain_id = chain.id.clone();
                        if let Err(error) =
                            sync_live_chain_runtime(&project_runtime, session, &chain_id)
                        {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                        replace_project_chains(
                            &project_chains,
                            &session.project,
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
                    set_status_info(&window, &toast_timer, "Preset removido.");
                }
                Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
            }
        });
    }
}
