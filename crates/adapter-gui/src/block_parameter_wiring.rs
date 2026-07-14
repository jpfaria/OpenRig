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

use rfd::FileDialog;
use slint::{ComponentHandle, SharedString, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::block_editor::{
    block_editor_data_with_selected, block_parameter_extensions, block_parameter_items_for_editor,
    build_params_from_items, internal_block_parameter_value, schedule_block_editor_persist,
    set_block_parameter_bool, set_block_parameter_number, set_block_parameter_option,
    set_block_parameter_text,
};
use crate::eq::{compute_eq_curves, eq_viz_sample_rate};
use crate::helpers::log_gui_message;
use crate::project_ops::sync_project_dirty;
use crate::project_view::{
    block_model_index, block_model_picker_items, block_model_picker_labels, block_type_index,
    replace_project_chains,
};
use crate::runtime_lifecycle::sync_live_chain_runtime;
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem, ProjectChainItem,
    SELECT_SELECTED_BLOCK_ID,
};

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

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: BlockParameterCtx,
) {
    let BlockParameterCtx {
        block_editor_draft,
        block_parameter_items,
        block_model_options,
        block_model_option_labels,
        eq_band_curves,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        block_editor_persist_timer,
        input_chain_devices,
        output_chain_devices,
        auto_save,
    } = ctx;
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
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_toggle_block_drawer_enabled(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = block_editor_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.enabled = !draft.enabled;
            log::info!("[toggle_block_drawer_enabled] chain_index={}, block_index={:?}, enabled={}, effect_type='{}', model_id='{}'",
                draft.chain_index, draft.block_index, draft.enabled, draft.effect_type, draft.model_id);
            window.set_block_drawer_enabled(draft.enabled);
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_enabled(draft.enabled);
            }
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
                    "block-drawer.toggle-enabled",
                auto_save,
                );
            }
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
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_text(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Update UI immediately for visual feedback.
            set_block_parameter_text(&block_parameter_items, path.as_str(), value.as_str());
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }

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
        let eq_band_curves = eq_band_curves.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_number(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_number(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }
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
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_bool(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Update UI immediately for visual feedback.
            set_block_parameter_bool(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }

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
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let select_block_model_options = block_model_options.clone();
        let select_block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_select_block_parameter_option(move |path, index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Update UI item (sets selected_option_index and value_text).
            set_block_parameter_option(&block_parameter_items, path.as_str(), index);

            // Special handling for SelectBlock's own block-selector param
            // (SELECT_SELECTED_BLOCK_ID): this re-renders the sub-block editor
            // and is UI-only — no project mutation needed here.
            if path.as_str() == SELECT_SELECTED_BLOCK_ID {
                let selected_option_block_id = internal_block_parameter_value(
                    &block_parameter_items,
                    SELECT_SELECTED_BLOCK_ID,
                );
                if let (Some(draft), Some(selected_option_block_id)) = (
                    block_editor_draft.borrow_mut().as_mut(),
                    selected_option_block_id,
                ) {
                    if draft.is_select {
                        if let Some(session) = project_session.borrow().as_ref() {
                            if let Some(block_index) = draft.block_index {
                                let proj = session.project.borrow();
                                if let Some(block) = proj
                                    .chains
                                    .get(draft.chain_index)
                                    .and_then(|chain| chain.blocks.get(block_index))
                                {
                                    if let Some(editor_data) = block_editor_data_with_selected(
                                        block,
                                        Some(&selected_option_block_id),
                                    ) {
                                        draft.effect_type = editor_data.effect_type.clone();
                                        draft.model_id = editor_data.model_id.clone();
                                        let items = block_model_picker_items(
                                            &editor_data.effect_type,
                                            &draft.instrument,
                                        );
                                        select_block_model_option_labels
                                            .set_vec(block_model_picker_labels(&items));
                                        select_block_model_options.set_vec(items);
                                        block_parameter_items.set_vec(
                                            block_parameter_items_for_editor(&editor_data),
                                        );
                                        window.set_block_drawer_selected_type_index(
                                            block_type_index(
                                                &editor_data.effect_type,
                                                &draft.instrument,
                                            ),
                                        );
                                        window.set_block_drawer_selected_model_index(
                                            block_model_index(
                                                &editor_data.effect_type,
                                                &editor_data.model_id,
                                                &draft.instrument,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                window.set_block_drawer_status_message("".into());
                return;
            }

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

            // Resolve chain_id / block_id and the option string value from project.
            let (chain_id, block_id, option_value) = {
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
                // The option string was already written to value_text by
                // set_block_parameter_option above — read it back from the model.
                let opt_val = internal_block_parameter_value(&block_parameter_items, path.as_str())
                    .unwrap_or_default();
                (chain.id.clone(), block.id.clone(), opt_val)
            };

            // Dispatch — mutates project via the shared Rc<RefCell<Project>>.
            let dispatch_ok = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                match session
                    .dispatcher
                    .dispatch(Command::SelectBlockParameterOption {
                        chain: chain_id.clone(),
                        block: block_id,
                        path: path.to_string(),
                        value: option_value,
                        index: index as usize,
                    }) {
                    Ok(events) => events
                        .into_iter()
                        .any(|e| matches!(e, Event::BlockParameterChanged { .. })),
                    Err(e) => {
                        log::error!("[adapter-gui] block-drawer.option dispatch: {e}");
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
                log::error!("[adapter-gui] block-drawer.option runtime sync: {e}");
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
        window.on_pick_block_parameter_file(move |path| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Open native file dialog — UI concern; kept in adapter.
            let extensions = block_parameter_extensions(&block_parameter_items, path.as_str());
            let mut dialog = FileDialog::new();
            if !extensions.is_empty() {
                let refs = extensions
                    .iter()
                    .map(|value| value.as_str())
                    .collect::<Vec<_>>();
                dialog = dialog.add_filter("Arquivos suportados", &refs);
            }
            let Some(file) = dialog.pick_file() else {
                return;
            };

            // Update UI immediately.
            set_block_parameter_text(
                &block_parameter_items,
                path.as_str(),
                file.to_string_lossy().as_ref(),
            );
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
                match session
                    .dispatcher
                    .dispatch(Command::PickBlockParameterFile {
                        chain: chain_id.clone(),
                        block: block_id,
                        path: path.to_string(),
                        file: file.clone(),
                    }) {
                    Ok(events) => events
                        .into_iter()
                        .any(|e| matches!(e, Event::BlockParameterChanged { .. })),
                    Err(e) => {
                        log::error!("[adapter-gui] block-drawer.file dispatch: {e}");
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
                log::error!("[adapter-gui] block-drawer.file runtime sync: {e}");
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
