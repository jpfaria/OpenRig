//! All callbacks registered on the per-instance `ChainEditorWindow`.
//!
//! Owns ~30 `editor_window.on_*` callbacks: chain name + instrument editing,
//! I/O group edit/add/remove (input + output), save/cancel of the chain as a
//! whole, and the inline I/O endpoint editor (select-device, toggle-channel,
//! select-mode, save, cancel) for both input and output sides. Save commits
//! the draft via `chain_from_draft`, validates channel conflicts, resyncs
//! the live runtime, and refreshes the chain rows.
//!
//! Called once per editor instance from `chain_crud_wiring::wire` (which
//! creates a fresh `ChainEditorWindow` on add/configure).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use domain::ids::{BlockId, DeviceId};
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{ChainInputMode, ChainOutputMode};

use crate::audio_devices::{refresh_input_devices, refresh_output_devices, selected_device_index};
use crate::chain_editor::{
    chain_from_draft, input_mode_from_index, input_mode_to_index, instrument_index_to_string,
    output_mode_from_index, output_mode_to_index,
};
use crate::helpers::clear_status;
use crate::io_groups::apply_chain_io_groups;
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::{
    ChainDraft, InputGroupDraft, IoBlockInsertDraft, OutputGroupDraft, ProjectSession,
};
use crate::sync_live_chain_runtime;
use crate::{
    AppWindow, ChainEditorWindow, ChainInputWindow, ChainOutputWindow, ChannelOptionItem,
    ProjectChainItem,
};

pub(crate) fn setup_chain_editor_callbacks(
    editor_window: &ChainEditorWindow,
    weak_window: slint::Weak<AppWindow>,
    chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    chain_input_device_options: Rc<VecModel<SharedString>>,
    chain_output_device_options: Rc<VecModel<SharedString>>,
    chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
    _weak_input_window: slint::Weak<ChainInputWindow>,
    _weak_output_window: slint::Weak<ChainOutputWindow>,
    io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
    toast_timer: Rc<Timer>,
    auto_save: bool,
) {
    // on_update_chain_name
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        editor_window.on_update_chain_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.name = value.to_string();
                window.set_chain_draft_name(value.clone());
                chain_window.set_chain_name(value);
            }
        });
    }
    // on_select_instrument
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_select_instrument(move |index| {
            let instrument = instrument_index_to_string(index).to_string();
            log::debug!("[select_instrument] index={}, instrument='{}'", index, instrument);
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.instrument = instrument;
                log::debug!("[select_instrument] draft updated to '{}'", draft.instrument);
            } else {
                log::warn!("[select_instrument] no draft to update!");
            }
        });
    }
    // on_edit_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        editor_window.on_edit_input(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_input_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_io_groups(
                    &window,
                    &chain_window,
                    draft,
                    &fresh_input,
                    &fresh_output,
                );
                let device_opts = ModelRc::from(chain_input_device_options.clone());
                let channel_opts = ModelRc::from(chain_input_channels.clone());
                chain_window.set_input_device_options(device_opts.clone());
                chain_window.set_input_channels(channel_opts.clone());
                let mut dev_idx = -1i32;
                let mut mode_idx = 0i32;
                if let Some(input_group) = draft.inputs.get(gi) {
                    dev_idx = selected_device_index(
                        &fresh_input,
                        input_group.device_id.as_deref(),
                    );
                    mode_idx = input_mode_to_index(input_group.mode);
                    chain_window.set_input_selected_device_index(dev_idx);
                    chain_window.set_input_mode_index(mode_idx);
                }
                chain_window.set_input_editor_status("".into());
                chain_window.set_show_input_editor(true);
                // Fullscreen: propagate to main window inline I/O editor
                if window.get_fullscreen() {
                    let labels: Vec<slint::SharedString> = vec!["Mono".into(), "Stereo".into(), "Dual Mono".into()];
                    let device_strings: Vec<slint::SharedString> = fresh_input.iter().map(|d| slint::SharedString::from(d.name.as_str())).collect();
                    window.set_chain_io_editor_title("Entrada".into());
                    window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(device_strings))));
                    window.set_chain_io_selected_device_index(dev_idx);
                    window.set_chain_io_channels(channel_opts);
                    window.set_chain_io_editor_status("".into());
                    window.set_chain_io_show_mode_selector(true);
                    window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                    window.set_chain_io_selected_mode_index(mode_idx);
                    window.set_show_chain_io_editor(true);
                }
            }
        });
    }
    // on_edit_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        editor_window.on_edit_output(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_output_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_io_groups(
                    &window,
                    &chain_window,
                    draft,
                    &fresh_input,
                    &fresh_output,
                );
                let device_opts = ModelRc::from(chain_output_device_options.clone());
                let channel_opts = ModelRc::from(chain_output_channels.clone());
                chain_window.set_output_device_options(device_opts.clone());
                chain_window.set_output_channels(channel_opts.clone());
                let mut dev_idx = -1i32;
                let mut mode_idx = 0i32;
                if let Some(output_group) = draft.outputs.get(gi) {
                    dev_idx = selected_device_index(
                        &fresh_output,
                        output_group.device_id.as_deref(),
                    );
                    mode_idx = output_mode_to_index(output_group.mode);
                    chain_window.set_output_selected_device_index(dev_idx);
                    chain_window.set_output_mode_index(mode_idx);
                }
                chain_window.set_output_editor_status("".into());
                chain_window.set_show_output_editor(true);
                // Fullscreen: propagate to main window inline I/O editor
                if window.get_fullscreen() {
                    let labels: Vec<slint::SharedString> = vec!["Mono".into(), "Stereo".into()];
                    let device_strings: Vec<slint::SharedString> = fresh_output.iter().map(|d| slint::SharedString::from(d.name.as_str())).collect();
                    window.set_chain_io_editor_title("Saída".into());
                    window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(device_strings))));
                    window.set_chain_io_selected_device_index(dev_idx);
                    window.set_chain_io_channels(channel_opts);
                    window.set_chain_io_editor_status("".into());
                    window.set_chain_io_show_mode_selector(true);
                    window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                    window.set_chain_io_selected_mode_index(mode_idx);
                    window.set_show_chain_io_editor(true);
                }
            }
        });
    }
    // on_add_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        editor_window.on_add_input(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.inputs.len();
                draft.inputs.push(InputGroupDraft {
                    device_id: None,
                    channels: Vec::new(),
                    mode: ChainInputMode::Mono,
                });
                draft.editing_input_index = Some(idx);
                draft.adding_new_input = true;
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_io_groups(
                        &window,
                        &chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                chain_window.set_input_device_options(ModelRc::from(chain_input_device_options.clone()));
                chain_window.set_input_channels(ModelRc::from(chain_input_channels.clone()));
                if let Some(input_group) = draft.inputs.get(new_idx) {
                    chain_window.set_input_selected_device_index(selected_device_index(
                        &fresh_input,
                        input_group.device_id.as_deref(),
                    ));
                    chain_window.set_input_mode_index(input_mode_to_index(input_group.mode));
                }
                chain_window.set_input_editor_status("".into());
                chain_window.set_show_input_editor(true);
            }
        });
    }
    // on_add_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        editor_window.on_add_output(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.outputs.len();
                draft.outputs.push(OutputGroupDraft {
                    device_id: None,
                    channels: Vec::new(),
                    mode: ChainOutputMode::Stereo,
                });
                draft.editing_output_index = Some(idx);
                draft.adding_new_output = true;
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_io_groups(
                        &window,
                        &chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                chain_window.set_output_device_options(ModelRc::from(chain_output_device_options.clone()));
                chain_window.set_output_channels(ModelRc::from(chain_output_channels.clone()));
                if let Some(output_group) = draft.outputs.get(new_idx) {
                    chain_window.set_output_selected_device_index(selected_device_index(
                        &fresh_output,
                        output_group.device_id.as_deref(),
                    ));
                    chain_window.set_output_mode_index(output_mode_to_index(output_group.mode));
                }
                chain_window.set_output_editor_status("".into());
                chain_window.set_show_output_editor(true);
            }
        });
    }
    // on_remove_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_remove_input(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.inputs.len() <= 1 {
                return;
            }
            let gi = group_index as usize;
            if gi < draft.inputs.len() {
                draft.inputs.remove(gi);
                // Reset editing index if it was pointing to the removed group
                if draft.editing_input_index == Some(gi) {
                    draft.editing_input_index = None;
                } else if let Some(idx) = draft.editing_input_index {
                    if idx > gi {
                        draft.editing_input_index = Some(idx - 1);
                    }
                }
            }
            apply_chain_io_groups(
                &window,
                &chain_window,
                draft,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
        });
    }
    // on_remove_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_remove_output(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.outputs.len() <= 1 {
                return;
            }
            let gi = group_index as usize;
            if gi < draft.outputs.len() {
                draft.outputs.remove(gi);
                if draft.editing_output_index == Some(gi) {
                    draft.editing_output_index = None;
                } else if let Some(idx) = draft.editing_output_index {
                    if idx > gi {
                        draft.editing_output_index = Some(idx - 1);
                    }
                }
            }
            apply_chain_io_groups(
                &window,
                &chain_window,
                draft,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
        });
    }
    // on_save_chain
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        editor_window.on_save_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                chain_window.set_status_message(rust_i18n::t!("Nenhum projeto carregado.").to_string().into());
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    chain_window.set_status_message(rust_i18n::t!("Nenhuma chain em edição.").to_string().into());
                    return;
                }
            };
            if draft.inputs.is_empty() {
                chain_window.set_status_message(rust_i18n::t!("Adicione pelo menos uma entrada.").to_string().into());
                return;
            }
            if draft.outputs.is_empty() {
                chain_window.set_status_message(rust_i18n::t!("Adicione pelo menos uma saída.").to_string().into());
                return;
            }
            for (i, input) in draft.inputs.iter().enumerate() {
                if input.device_id.is_none() {
                    chain_window.set_status_message(format!("Entrada {}: selecione o dispositivo.", i + 1).into());
                    return;
                }
                if input.channels.is_empty() {
                    chain_window.set_status_message(format!("Entrada {}: selecione pelo menos um canal.", i + 1).into());
                    return;
                }
            }
            for (i, output) in draft.outputs.iter().enumerate() {
                if output.device_id.is_none() {
                    chain_window.set_status_message(format!("Saída {}: selecione o dispositivo.", i + 1).into());
                    return;
                }
                if output.channels.is_empty() {
                    chain_window.set_status_message(format!("Saída {}: selecione pelo menos um canal.", i + 1).into());
                    return;
                }
            }
            let editing_index = draft.editing_index;
            log::debug!("[save_chain] editing_index={:?}, draft.instrument='{}'", editing_index, draft.instrument);
            let existing_chain =
                editing_index.and_then(|index| session.project.chains.get(index).cloned());
            let chain = chain_from_draft(&draft, existing_chain.as_ref());
            if let Err(msg) = chain.validate_channel_conflicts() {
                chain_window.set_status_message(msg.into());
                return;
            }
            log::info!("=== CHAIN SAVED: id='{}', name={:?}, instrument='{}', editing={:?} ===",
                chain.id.0, chain.description, chain.instrument, editing_index);
            let chain_id = chain.id.clone();
            if let Some(index) = editing_index {
                if let Some(current) = session.project.chains.get_mut(index) {
                    *current = chain;
                }
            } else {
                session.project.chains.push(chain);
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                chain_window.set_status_message(error.to_string().into());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            *chain_draft.borrow_mut() = None;
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            chain_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
            let _ = chain_window.hide();
        });
    }
    // on_cancel_chain
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let toast_timer = toast_timer.clone();
        editor_window.on_cancel_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            chain_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
            let _ = chain_window.hide();
        });
    }
    // inline input editor: on_input_select_device
    {
        let weak_window = weak_window.clone();
        editor_window.on_input_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_input_device(index);
            }
        });
    }
    // inline input editor: on_input_toggle_channel
    {
        let weak_window = weak_window.clone();
        editor_window.on_input_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_input_channel(index, selected);
            }
        });
    }
    // inline input editor: on_input_select_mode
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_input_select_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_input_index {
                    if let Some(input) = draft.inputs.get_mut(gi) {
                        input.mode = input_mode_from_index(index);
                    }
                }
            }
        });
    }
    // inline input editor: on_input_cancel
    {
        let weak_chain_window = editor_window.as_weak();
        let weak_window = weak_window.clone();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_input_cancel(move || {
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            chain_window.set_input_editor_status("".into());
            chain_window.set_show_input_editor(false);
            if io_block_insert_draft.borrow().is_some() {
                *io_block_insert_draft.borrow_mut() = None;
                *chain_draft.borrow_mut() = None;
                return;
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_input {
                    if let Some(idx) = draft.editing_input_index {
                        if idx < draft.inputs.len() {
                            draft.inputs.remove(idx);
                        }
                    }
                    draft.adding_new_input = false;
                    draft.editing_input_index = None;
                    if let Some(window) = weak_window.upgrade() {
                        apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    }
                }
            }
        });
    }
    // inline input editor: on_input_save
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_input_save(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            let io_insert = io_block_insert_draft.borrow().clone();
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "input" {
                    let input_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_input_editor(false);
                            return;
                        };
                        let Some(ig) = draft.inputs.first().cloned() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_input_editor(false);
                            return;
                        };
                        ig
                    };
                    if input_group.device_id.is_none() || input_group.channels.is_empty() {
                        chain_window.set_input_editor_status("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    *io_block_insert_draft.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        chain_window.set_show_input_editor(false);
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        chain_window.set_show_input_editor(false);
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let input_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId(input_group.device_id.clone().unwrap_or_default()),
                                mode: input_group.mode,
                                channels: input_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, input_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    chain_window.set_input_editor_status("".into());
                    chain_window.set_show_input_editor(false);
                    return;
                }
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                chain_window.set_show_input_editor(false);
                return;
            };
            let Some(gi) = draft.editing_input_index else {
                chain_window.set_show_input_editor(false);
                return;
            };
            let Some(input_group) = draft.inputs.get(gi) else {
                chain_window.set_show_input_editor(false);
                return;
            };
            if input_group.device_id.is_none() || input_group.channels.is_empty() {
                chain_window.set_input_editor_status("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else { return; };
                let Some(chain) = session.project.chains.get_mut(index) else { return; };
                let new_input_blocks: Vec<AudioBlock> = draft.inputs.iter().enumerate().map(|(i, ig)| AudioBlock {
                    id: BlockId(format!("{}:input:{}", chain.id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                            mode: ig.mode,
                            channels: ig.channels.clone(),
                        }],
                    }),
                }).collect();
                let non_input_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(new_input_blocks.len() + non_input_blocks.len());
                all_blocks.extend(new_input_blocks);
                all_blocks.extend(non_input_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("input editor save error: {error}");
                    return;
                }
                replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            draft.adding_new_input = false;
            chain_window.set_input_editor_status("".into());
            chain_window.set_show_input_editor(false);
        });
    }
    // inline output editor: on_output_select_device
    {
        let weak_window = weak_window.clone();
        editor_window.on_output_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_output_device(index);
            }
        });
    }
    // inline output editor: on_output_toggle_channel
    {
        let weak_window = weak_window.clone();
        editor_window.on_output_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_output_channel(index, selected);
            }
        });
    }
    // inline output editor: on_output_select_mode
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_output_select_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_output_index {
                    if let Some(output) = draft.outputs.get_mut(gi) {
                        output.mode = output_mode_from_index(index);
                    }
                }
            }
        });
    }
    // inline output editor: on_output_cancel
    {
        let weak_chain_window = editor_window.as_weak();
        let weak_window = weak_window.clone();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_output_cancel(move || {
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            chain_window.set_output_editor_status("".into());
            chain_window.set_show_output_editor(false);
            if io_block_insert_draft.borrow().is_some() {
                *io_block_insert_draft.borrow_mut() = None;
                *chain_draft.borrow_mut() = None;
                return;
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_output {
                    if let Some(idx) = draft.editing_output_index {
                        if idx < draft.outputs.len() {
                            draft.outputs.remove(idx);
                        }
                    }
                    draft.adding_new_output = false;
                    draft.editing_output_index = None;
                    if let Some(window) = weak_window.upgrade() {
                        apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    }
                }
            }
        });
    }
    // inline output editor: on_output_save
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_output_save(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            let io_insert = io_block_insert_draft.borrow().clone();
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "output" {
                    let output_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_output_editor(false);
                            return;
                        };
                        let Some(og) = draft.outputs.first().cloned() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_output_editor(false);
                            return;
                        };
                        og
                    };
                    if output_group.device_id.is_none() || output_group.channels.is_empty() {
                        chain_window.set_output_editor_status("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    *io_block_insert_draft.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        chain_window.set_show_output_editor(false);
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        chain_window.set_show_output_editor(false);
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let output_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries: vec![OutputEntry {
                                device_id: DeviceId(output_group.device_id.clone().unwrap_or_default()),
                                mode: output_group.mode,
                                channels: output_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, output_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    chain_window.set_output_editor_status("".into());
                    chain_window.set_show_output_editor(false);
                    return;
                }
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                chain_window.set_show_output_editor(false);
                return;
            };
            let Some(gi) = draft.editing_output_index else {
                chain_window.set_show_output_editor(false);
                return;
            };
            let Some(output_group) = draft.outputs.get(gi) else {
                chain_window.set_show_output_editor(false);
                return;
            };
            if output_group.device_id.is_none() || output_group.channels.is_empty() {
                chain_window.set_output_editor_status("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else { return; };
                let Some(chain) = session.project.chains.get_mut(index) else { return; };
                let new_output_blocks: Vec<AudioBlock> = draft.outputs.iter().enumerate().map(|(i, og)| AudioBlock {
                    id: BlockId(format!("{}:output:{}", chain.id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                            mode: og.mode,
                            channels: og.channels.clone(),
                        }],
                    }),
                }).collect();
                let non_output_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Output(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(non_output_blocks.len() + new_output_blocks.len());
                all_blocks.extend(non_output_blocks);
                all_blocks.extend(new_output_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("output editor save error: {error}");
                    return;
                }
                replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            draft.adding_new_output = false;
            chain_window.set_output_editor_status("".into());
            chain_window.set_show_output_editor(false);
        });
    }
}
