//! Wiring for the block-drawer parameter edit callbacks.
//!
//! Owns the 7 callbacks that mutate parameters of the active block_editor_draft:
//! - on_update_block_parameter_number_text — typed text → parsed f32 (dispatches Command)
//! - on_toggle_block_drawer_enabled       — toggle the block's enabled flag
//! - on_update_block_parameter_number     — numeric params (draft-based persist)
//! - on_update_block_parameter_bool       — bool params (dispatches Command)
//! - on_update_block_parameter_text       — string params (dispatches Command)
//! - on_select_block_parameter_option     — enum/select params (dispatches Command)
//! - on_pick_block_parameter_file         — file-picker params (dispatches Command)
//!
//! Migrated to dispatch via `LocalDispatcher`:
//! - `on_update_block_parameter_number_text` → `Command::SetBlockParameterNumber`
//! - `on_update_block_parameter_bool`        → `Command::SetBlockParameterBool`
//! - `on_update_block_parameter_text`        → `Command::SetBlockParameterText`
//! - `on_select_block_parameter_option`      → `Command::SelectBlockParameterOption`
//! - `on_pick_block_parameter_file`          → `Command::PickBlockParameterFile`

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::block_editor::{
    build_params_from_items, schedule_block_editor_persist,
    set_block_parameter_bool, set_block_parameter_number, set_block_parameter_text,
};
use crate::eq::{compute_eq_curves, eq_viz_sample_rate};
use crate::helpers::log_gui_message;
use crate::project_ops::sync_project_dirty;
use crate::project_view::{
    replace_project_chains,
};
use crate::runtime_lifecycle::sync_live_chain_runtime;
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::{AppWindow, BlockModelPickerItem, BlockParameterItem, ProjectChainItem};

pub(crate) struct BlockParameterCtx {
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub block_model_option_labels: Rc<VecModel<SharedString>>,
    pub eq_band_curves: Rc<VecModel<SharedString>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub block_editor_persist_timer: Rc<Timer>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub auto_save: bool,
}


pub(crate) fn wire(window: &AppWindow, ctx: BlockParameterCtx) {
    wire_numeric_params(window, &ctx);
    wire_text_bool_params(window, &ctx);
    crate::block_parameter_extras::wire_select_param(window, &ctx);
    crate::block_parameter_extras::wire_toggle_and_file(window, &ctx);
}

fn wire_numeric_params(window: &AppWindow, ctx: &BlockParameterCtx) {
    let block_editor_draft = &ctx.block_editor_draft;
    let block_parameter_items = &ctx.block_parameter_items;
    let eq_band_curves = &ctx.eq_band_curves;
    let project_session = &ctx.project_session;
    let project_chains = &ctx.project_chains;
    let project_runtime = &ctx.project_runtime;
    let saved_project_snapshot = &ctx.saved_project_snapshot;
    let project_dirty = &ctx.project_dirty;
    let block_editor_persist_timer = &ctx.block_editor_persist_timer;
    let input_chain_devices = &ctx.input_chain_devices;
    let output_chain_devices = &ctx.output_chain_devices;
    let auto_save = ctx.auto_save;

    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_number_text(move |path, value_text| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Parse the text value — adapter (UI) concern; kept here.
            let normalized = value_text.replace(',', ".");
            let Ok(value) = normalized.parse::<f64>() else {
                log_gui_message("block-drawer.number-text", "Valor numérico inválido.");
                return;
            };
            // Update the UI row immediately for visual feedback.
            set_block_parameter_number(&block_parameter_items, path.as_str(), value as f32);
            window.set_block_drawer_status_message("".into());

            // Only dispatch if editing an existing block (draft.block_index.is_some()).
            let (chain_index, block_index) = {
                let draft_borrow = block_editor_draft.borrow();
                let Some(draft) = draft_borrow.as_ref() else {
                    return;
                };
                let Some(bi) = draft.block_index else {
                    return;
                };
                (draft.chain_index, bi)
            };

            // Resolve chain_id / block_id from project indices.
            let (chain_id, block_id) = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_index) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };

            // Dispatch — mutates project via the shared Rc<RefCell<Project>>.
            let dispatch_ok = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                match session
                    .dispatcher
                    .dispatch(Command::SetBlockParameterNumber {
                        chain: chain_id.clone(),
                        block: block_id,
                        path: path.to_string(),
                        value,
                    }) {
                    Ok(events) => events
                        .into_iter()
                        .any(|e| matches!(e, Event::BlockParameterChanged { .. })),
                    Err(e) => {
                        log::error!("[adapter-gui] block-drawer.number-text dispatch: {e}");
                        window.set_block_drawer_status_message(e.to_string().into());
                        return;
                    }
                }
            };
            if !dispatch_ok {
                return;
            }

            // Sync audio runtime + refresh UI + mark dirty.
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[adapter-gui] block-drawer.number-text runtime sync: {e}");
                window.set_block_drawer_status_message(e.to_string().into());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
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
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let eq_band_curves = eq_band_curves.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_number(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_number(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
                let params = build_params_from_items(&block_parameter_items);
                let (eq_total, eq_bands) = compute_eq_curves(
                    &draft.effect_type,
                    &draft.model_id,
                    &params,
                    eq_viz_sample_rate(&project_runtime),
                );
                eq_band_curves.set_vec(
                    eq_bands
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                );
                window.set_eq_total_curve(eq_total.into());
                if draft.block_index.is_some() {
                    schedule_block_editor_persist(
                        &block_editor_persist_timer,
                        weak_window.clone(),
                        block_editor_draft.clone(),
                        block_parameter_items.clone(),
                        project_session.clone(),
                        project_chains.clone(),
                        project_runtime.clone(),
                        saved_project_snapshot.clone(),
                        project_dirty.clone(),
                        input_chain_devices.clone(),
                        output_chain_devices.clone(),
                        "block-drawer.number",
                        auto_save,
                    );
                }
            }
        });
    }

}

fn wire_text_bool_params(window: &AppWindow, ctx: &BlockParameterCtx) {
    let block_editor_draft = &ctx.block_editor_draft;
    let block_parameter_items = &ctx.block_parameter_items;
    let project_session = &ctx.project_session;
    let project_chains = &ctx.project_chains;
    let project_runtime = &ctx.project_runtime;
    let saved_project_snapshot = &ctx.saved_project_snapshot;
    let project_dirty = &ctx.project_dirty;
    let input_chain_devices = &ctx.input_chain_devices;
    let output_chain_devices = &ctx.output_chain_devices;
    let auto_save = ctx.auto_save;

    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_text(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Update UI immediately for visual feedback.
            set_block_parameter_text(&block_parameter_items, path.as_str(), value.as_str());
            window.set_block_drawer_status_message("".into());

            // Only dispatch if editing an existing block.
            let (chain_index, block_index) = {
                let draft_borrow = block_editor_draft.borrow();
                let Some(draft) = draft_borrow.as_ref() else {
                    return;
                };
                let Some(bi) = draft.block_index else {
                    return;
                };
                (draft.chain_index, bi)
            };

            // Resolve chain_id / block_id from project indices.
            let (chain_id, block_id) = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_index) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };

            // Dispatch — mutates project via the shared Rc<RefCell<Project>>.
            let dispatch_ok = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                match session.dispatcher.dispatch(Command::SetBlockParameterText {
                    chain: chain_id.clone(),
                    block: block_id,
                    path: path.to_string(),
                    value: value.to_string(),
                }) {
                    Ok(events) => events
                        .into_iter()
                        .any(|e| matches!(e, Event::BlockParameterChanged { .. })),
                    Err(e) => {
                        log::error!("[adapter-gui] block-drawer.text dispatch: {e}");
                        window.set_block_drawer_status_message(e.to_string().into());
                        return;
                    }
                }
            };
            if !dispatch_ok {
                return;
            }

            // Sync audio runtime + refresh UI + mark dirty.
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[adapter-gui] block-drawer.text runtime sync: {e}");
                window.set_block_drawer_status_message(e.to_string().into());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
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
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_bool(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Update UI immediately for visual feedback.
            set_block_parameter_bool(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());

            // Only dispatch if editing an existing block.
            let (chain_index, block_index) = {
                let draft_borrow = block_editor_draft.borrow();
                let Some(draft) = draft_borrow.as_ref() else {
                    return;
                };
                let Some(bi) = draft.block_index else {
                    return;
                };
                (draft.chain_index, bi)
            };

            // Resolve chain_id / block_id from project indices.
            let (chain_id, block_id) = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_index) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };

            // Dispatch — mutates project via the shared Rc<RefCell<Project>>.
            let dispatch_ok = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                match session.dispatcher.dispatch(Command::SetBlockParameterBool {
                    chain: chain_id.clone(),
                    block: block_id,
                    path: path.to_string(),
                    value,
                }) {
                    Ok(events) => events
                        .into_iter()
                        .any(|e| matches!(e, Event::BlockParameterChanged { .. })),
                    Err(e) => {
                        log::error!("[adapter-gui] block-drawer.bool dispatch: {e}");
                        window.set_block_drawer_status_message(e.to_string().into());
                        return;
                    }
                }
            };
            if !dispatch_ok {
                return;
            }

            // Sync audio runtime + refresh UI + mark dirty.
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[adapter-gui] block-drawer.bool runtime sync: {e}");
                window.set_block_drawer_status_message(e.to_string().into());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
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

}

