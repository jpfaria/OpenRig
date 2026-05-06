//! Wiring for the block-drawer parameter edit callbacks.
//!
//! Owns the 7 callbacks that mutate parameters of the active block_editor_draft:
//! - on_update_block_parameter_number_text — typed text → parsed f32
//! - on_toggle_block_drawer_enabled       — toggle the block's enabled flag
//! - on_update_block_parameter_text       — string params
//! - on_update_block_parameter_number     — numeric params
//! - on_update_block_parameter_bool       — bool params
//! - on_select_block_parameter_option     — enum/select params
//! - on_pick_block_parameter_file         — file-picker params
//!
//! Each one updates the in-memory ParameterSet via the block_editor helpers
//! and schedules a persist via schedule_block_editor_persist.

use std::cell::RefCell;
use std::rc::Rc;

use rfd::FileDialog;
use slint::{ComponentHandle, SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::block_editor::{
    block_editor_data_with_selected, block_parameter_extensions, block_parameter_items_for_editor,
    build_params_from_items, internal_block_parameter_value, schedule_block_editor_persist,
    set_block_parameter_bool, set_block_parameter_number, set_block_parameter_option,
    set_block_parameter_text,
};
use crate::eq::compute_eq_curves;
use crate::helpers::log_gui_message;
use crate::project_view::{
    block_model_index, block_model_picker_items, block_model_picker_labels, block_type_index,
};
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_number_text(move |path, value_text| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let normalized = value_text.replace(',', ".");
            let Ok(value) = normalized.parse::<f32>() else {
                log_gui_message("block-drawer.number-text", "Valor numérico inválido.");
                return;
            };
            set_block_parameter_number(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.number-text",
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_text(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_text(&block_parameter_items, path.as_str(), value.as_str());
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.text",
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
                let (eq_total, eq_bands) =
                    compute_eq_curves(&draft.effect_type, &draft.model_id, &params);
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_bool(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_bool(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.bool",
                        auto_save,
                    );
                }
            }
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_select_block_parameter_option(move |path, index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_option(&block_parameter_items, path.as_str(), index);
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
                                if let Some(block) = session
                                    .project
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
            }
            window.set_block_drawer_status_message("".into());
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.option",
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_pick_block_parameter_file(move |path| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
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
            set_block_parameter_text(
                &block_parameter_items,
                path.as_str(),
                file.to_string_lossy().as_ref(),
            );
            window.set_block_drawer_status_message("".into());
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.file",
                        auto_save,
                    );
                }
            }
        });
    }
}
