//! Wiring for the `ChainInputGroupsWindow` callbacks (7 callbacks).
//!
//! Owns the chain Input groups list — edit / remove / add a group plus the
//! save/cancel/toggle/delete actions for the IO block as a whole. Save
//! validates each input group has a device + at least one channel, then
//! writes the entries back into the target InputBlock and resyncs the live
//! runtime.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use domain::ids::DeviceId;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::{AudioBlockKind, InputEntry};
use project::chain::ChainInputMode;

use crate::audio_devices::{refresh_input_devices, refresh_output_devices};
use crate::helpers::show_child_window;
use crate::io_groups::{apply_chain_input_window_state, build_io_group_items};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::{ChainDraft, InputGroupDraft, ProjectSession};
use crate::sync_live_chain_runtime;
use crate::{
    AppWindow, ChainInputGroupsWindow, ChainInputWindow, ChannelOptionItem, ProjectChainItem,
};

pub(crate) struct ChainInputGroupsCtx {
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_input_window: &ChainInputWindow,
    chain_input_groups_window: &ChainInputGroupsWindow,
    ctx: ChainInputGroupsCtx,
) {
    let ChainInputGroupsCtx {
        chain_draft,
        project_session,
        chain_input_device_options,
        chain_output_device_options,
        chain_input_channels,
        input_chain_devices,
        output_chain_devices,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        auto_save,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        chain_input_groups_window.on_edit_group(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
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
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            if let Some(input_group) = draft.inputs.get(gi) {
                apply_chain_input_window_state(
                    &input_window,
                    input_group,
                    draft,
                    &session.project,
                    &fresh_input,
                    &chain_input_channels,
                );
            }
            show_child_window(window.window(), input_window.window());
        });
    }
    {
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_input_groups_window.on_remove_group(move |group_index| {
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.inputs.len() <= 1 {
                groups_window.set_status_message("É necessário pelo menos uma entrada.".into());
                return;
            }
            let gi = group_index as usize;
            if gi < draft.inputs.len() {
                draft.inputs.remove(gi);
                if draft.editing_input_index == Some(gi) {
                    draft.editing_input_index = None;
                } else if let Some(idx) = draft.editing_input_index {
                    if idx > gi {
                        draft.editing_input_index = Some(idx - 1);
                    }
                }
            }
            let (input_items, _) = build_io_group_items(
                draft,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
            groups_window.set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        chain_input_groups_window.on_add_group(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
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
                if let Some(groups_window) = weak_groups_window.upgrade() {
                    let (input_items, _) =
                        build_io_group_items(draft, &fresh_input, &fresh_output);
                    groups_window.set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            if let Some(input_group) = draft.inputs.get(new_idx) {
                apply_chain_input_window_state(
                    &input_window,
                    input_group,
                    draft,
                    &session.project,
                    &fresh_input,
                    &chain_input_channels,
                );
            }
            show_child_window(window.window(), input_window.window());
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_input_groups_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                groups_window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    groups_window.set_status_message("Nenhuma chain em edição.".into());
                    return;
                }
            };
            if draft.inputs.is_empty() {
                groups_window.set_status_message("Adicione pelo menos uma entrada.".into());
                return;
            }
            for (i, input) in draft.inputs.iter().enumerate() {
                if input.device_id.is_none() {
                    groups_window.set_status_message(
                        format!("Entrada {}: selecione o dispositivo.", i + 1).into(),
                    );
                    return;
                }
                if input.channels.is_empty() {
                    groups_window.set_status_message(
                        format!("Entrada {}: selecione pelo menos um canal.", i + 1).into(),
                    );
                    return;
                }
            }
            let editing_index = draft.editing_index;
            let io_block_idx = draft.editing_io_block_index;

            // Build new entries from draft
            let new_entries: Vec<InputEntry> = draft
                .inputs
                .iter()
                .filter(|ig| ig.device_id.is_some() && !ig.channels.is_empty())
                .map(|ig| InputEntry {
                    device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                    mode: ig.mode,
                    channels: ig.channels.clone(),
                })
                .collect();

            if let Some(chain_idx) = editing_index {
                if let Some(chain) = session.project.chains.get_mut(chain_idx) {
                    // Find target block: specific index or first InputBlock
                    let target_idx = io_block_idx.unwrap_or_else(|| {
                        chain
                            .blocks
                            .iter()
                            .position(|b| matches!(&b.kind, AudioBlockKind::Input(_)))
                            .unwrap_or(0)
                    });
                    if let Some(block) = chain.blocks.get_mut(target_idx) {
                        if let AudioBlockKind::Input(ref mut ib) = block.kind {
                            ib.entries = new_entries;
                        }
                    }
                    if let Err(msg) = chain.validate_channel_conflicts() {
                        groups_window.set_status_message(msg.into());
                        return;
                    }
                    let chain_id = chain.id.clone();
                    if let Err(error) =
                        sync_live_chain_runtime(&project_runtime, session, &chain_id)
                    {
                        groups_window.set_status_message(error.to_string().into());
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
                }
            }
            *chain_draft.borrow_mut() = None;
            groups_window.set_status_message("".into());
            let _ = groups_window.hide();
        });
    }
    {
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        chain_input_groups_window.on_cancel(move || {
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            let _ = groups_window.hide();
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        chain_input_groups_window.on_toggle_enabled(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(gw) = weak_groups_window.upgrade() else {
                return;
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let Some(chain_idx) = draft.editing_index else {
                return;
            };
            let Some(block_idx) = draft.editing_io_block_index else {
                return;
            };
            drop(draft_borrow);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else {
                return;
            };
            let Some(block) = chain.blocks.get_mut(block_idx) else {
                return;
            };
            block.enabled = !block.enabled;
            gw.set_block_enabled(block.enabled);
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("toggle I/O block enabled: {e}");
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
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        chain_input_groups_window.on_delete_block(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(gw) = weak_groups_window.upgrade() else {
                return;
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let Some(chain_idx) = draft.editing_index else {
                return;
            };
            let Some(block_idx) = draft.editing_io_block_index else {
                return;
            };
            drop(draft_borrow);
            *chain_draft.borrow_mut() = None;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else {
                return;
            };
            if block_idx < chain.blocks.len() {
                chain.blocks.remove(block_idx);
            }
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("delete I/O block: {e}");
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
            let _ = gw.hide();
        });
    }
}
