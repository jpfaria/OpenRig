//! `on_start_block_insert` and `on_choose_block_model` callbacks.
//!
//! * `on_start_block_insert` — opens the type picker for a new block at a
//!   given UI position. Translates the UI before-index (which excludes hidden
//!   I/O blocks) to the real `chain.blocks` index, seeds an empty
//!   `BlockEditorDraft`, and resets the block-drawer state.
//! * `on_choose_block_model` — when the user changes the active model in
//!   the editor, updates the draft, rebuilds parameter items / knob overlays
//!   / multi-slider / curve editor / EQ curves, and (when editing an existing
//!   block) schedules a debounced persist so the change is saved without a
//!   manual confirm.
//!
//! `on_choose_block_type` is its own concern — see `block_choose_type_callback`.
//!
//! Wired once from `run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::param::ParameterSet;

use crate::block_editor::{
    block_parameter_items_for_model, build_knob_overlays, schedule_block_editor_persist,
};
use crate::eq::{build_curve_editor_points, build_multi_slider_points, compute_eq_curves};
use crate::helpers::{sync_block_editor_window, use_inline_block_editor};
use crate::project_view::{block_model_picker_items, block_type_picker_items, set_selected_block};
use crate::state::{BlockEditorDraft, ProjectSession, SelectedBlock};
use crate::ui_index_to_real_block_index;
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem,
    BlockTypePickerItem, CurveEditorPoint, MultiSliderPoint, ProjectChainItem,
};

pub(crate) struct BlockInsertCallbacksCtx {
    pub selected_block: Rc<RefCell<Option<SelectedBlock>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub block_type_options: Rc<VecModel<BlockTypePickerItem>>,
    pub block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub block_model_option_labels: Rc<VecModel<SharedString>>,
    pub block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub multi_slider_points: Rc<VecModel<MultiSliderPoint>>,
    pub curve_editor_points: Rc<VecModel<CurveEditorPoint>>,
    pub eq_band_curves: Rc<VecModel<SharedString>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub block_editor_persist_timer: Rc<Timer>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: BlockInsertCallbacksCtx,
) {
    let BlockInsertCallbacksCtx {
        selected_block,
        block_editor_draft,
        block_type_options,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        block_editor_persist_timer,
        auto_save,
    } = ctx;

    // on_start_block_insert
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_type_options = block_type_options.clone();
        let block_model_options = block_model_options.clone();
        let filtered_block_model_options = filtered_block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let project_session = project_session.clone();
        window.on_start_block_insert(move |chain_index, before_index| {
            log::debug!("on_start_block_insert: chain_index={}, before_index={}", chain_index, before_index);
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let instrument = project_session.borrow().as_ref()
                .and_then(|s| {
                    let chain = s.project.chains.get(chain_index as usize)?;
                    log::info!("=== START_BLOCK_INSERT: chain_index={}, chain.instrument='{}', chain.description={:?} ===",
                        chain_index, chain.instrument, chain.description);
                    Some(chain.instrument.clone())
                })
                .unwrap_or_else(|| {
                    log::warn!("=== START_BLOCK_INSERT: no chain at index {}, defaulting to electric_guitar ===", chain_index);
                    block_core::DEFAULT_INSTRUMENT.to_string()
                });
            // Map UI before_index to real chain.blocks index (UI excludes hidden I/O blocks)
            let real_before_index = {
                let session_borrow = project_session.borrow();
                session_borrow.as_ref()
                    .and_then(|s| s.project.chains.get(chain_index as usize))
                    .map(|chain| ui_index_to_real_block_index(chain, before_index as usize))
                    .unwrap_or(before_index as usize)
            };
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = Some(BlockEditorDraft {
                chain_index: chain_index as usize,
                block_index: None,
                before_index: real_before_index,
                instrument: instrument.clone(),
                effect_type: String::new(),
                model_id: String::new(),
                enabled: true,
                is_select: false,
            });
            block_type_options.set_vec(block_type_picker_items(&instrument));
            block_model_options.set_vec(Vec::new());
            filtered_block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            set_selected_block(&window, None, None);
            window.set_block_drawer_edit_mode(false);
            window.set_block_drawer_selected_type_index(-1);
            window.set_block_drawer_selected_model_index(-1);
            window.set_block_drawer_status_message("".into());
            window.set_show_block_drawer(false);
            window.set_show_block_type_picker(true);
        });
    }

    // on_choose_block_model
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
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
        window.on_choose_block_model(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = block_editor_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let models = block_model_picker_items(&draft.effect_type, &draft.instrument);
            let Some(model) = models.get(index as usize) else {
                return;
            };
            log::debug!("on_choose_block_model: index={}, model_id='{}', effect_type='{}'", index, model.model_id, model.effect_type);
            draft.model_id = model.model_id.to_string();
            draft.effect_type = model.effect_type.to_string();
            let new_params = block_parameter_items_for_model(
                &model.effect_type,
                &model.model_id,
                &ParameterSet::default(),
            );
            let overlays = build_knob_overlays(project::catalog::model_knob_layout(&model.effect_type, &model.model_id), &new_params);
            block_parameter_items.set_vec(new_params);
            multi_slider_points.set_vec(build_multi_slider_points(&model.effect_type, &model.model_id, &ParameterSet::default()));
            curve_editor_points.set_vec(build_curve_editor_points(&model.effect_type, &model.model_id, &ParameterSet::default()));
            let (eq_total, eq_bands) = compute_eq_curves(&model.effect_type, &model.model_id, &ParameterSet::default());
            eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
            window.set_eq_total_curve(eq_total.into());
            window.set_block_drawer_selected_model_index(index);
            window.set_block_drawer_status_message("".into());
            if use_inline_block_editor(&window) {
                window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
            } else if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                sync_block_editor_window(&window, &block_editor_window);
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
                    "block-drawer.choose-model",
                    auto_save,
                );
            }
        });
    }
}
