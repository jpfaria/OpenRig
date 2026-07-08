//! `on_choose_block_type` — branches on the type chosen from the block-type
//! picker.
//!
//! * `insert`: creates an empty `InsertBlock` immediately (so it shows up in
//!   the chain), then opens the insert configuration window with a fresh
//!   `InsertDraft` for the user to fill in send/return endpoints.
//! * effect type (everything else): prefills the block drawer with the first
//!   available model, builds parameter items + knob overlays, and shows the
//!   inline drawer or the detached editor depending on capabilities.
//!
//! The `input` / `output` branch was removed in #716 — a chain's I/O is now
//! selected through the binding checklist, not by inserting I/O blocks.
//!
//! Wired once from `run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::chain::ChainInputMode;
use project::param::ParameterSet;

use crate::audio_devices::{
    refresh_input_devices, refresh_output_devices, replace_channel_options,
};
use crate::block_editor::{block_parameter_items_for_model, build_knob_overlays};
use crate::eq::{
    build_curve_editor_points, build_multi_slider_points, compute_eq_curves, eq_viz_sample_rate,
};
use crate::helpers::{show_child_window, sync_block_editor_window, use_inline_block_editor};
use crate::project_ops::sync_project_dirty;
use crate::project_view::{
    block_model_picker_items, block_model_picker_labels, block_type_picker_items,
    replace_project_chains,
};
use crate::state::{BlockEditorDraft, InsertDraft, ProjectSession};
use crate::sync_live_chain_runtime;
use crate::ui_state::block_drawer_state;
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem, ChainInsertWindow,
    ChannelOptionItem, CurveEditorPoint, MultiSliderPoint, ProjectChainItem,
};

pub(crate) struct BlockChooseTypeCallbackCtx {
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub insert_draft: Rc<RefCell<Option<InsertDraft>>>,
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
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub insert_send_channels: Rc<VecModel<ChannelOptionItem>>,
    pub insert_return_channels: Rc<VecModel<ChannelOptionItem>>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    chain_insert_window: &ChainInsertWindow,
    ctx: BlockChooseTypeCallbackCtx,
) {
    let BlockChooseTypeCallbackCtx {
        block_editor_draft,
        insert_draft,
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
        chain_input_device_options,
        chain_output_device_options,
        insert_send_channels,
        insert_return_channels,
        auto_save,
    } = ctx;

    let weak_window = window.as_weak();
    let weak_block_editor_window = block_editor_window.as_weak();
    let weak_insert_window = chain_insert_window.as_weak();

    window.on_choose_block_type(move |index| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let instrument = block_editor_draft
            .borrow()
            .as_ref()
            .map(|d| d.instrument.clone())
            .unwrap_or_else(|| block_core::DEFAULT_INSTRUMENT.to_string());
        let block_types = block_type_picker_items(&instrument);
        let Some(block_type) = block_types.get(index as usize) else {
            return;
        };
        log::debug!(
            "on_choose_block_type: index={}, type='{}', instrument='{}'",
            index,
            block_type.effect_type,
            instrument
        );

        // Handle I/O and Insert block types: open the dedicated window instead of the block editor
        let effect_type_str = block_type.effect_type.as_str();
        if effect_type_str == "insert" {
            // Insert block: create via Command::AddBlock so business logic stays in the dispatcher.
            let (chain_index, before_index) = {
                let draft_borrow = block_editor_draft.borrow();
                let Some(draft) = draft_borrow.as_ref() else {
                    return;
                };
                (draft.chain_index, draft.before_index)
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index) else {
                    return;
                };
                chain.id.clone()
            };
            // Dispatch Command::AddBlock — mutates project via shared Rc.
            if let Err(e) = session.dispatcher.dispatch(Command::AddBlock {
                chain: chain_id.clone(),
                kind: "insert".to_string(),
                model_id: "standard".to_string(),
                position: before_index,
            }) {
                log::error!("insert block AddBlock dispatch error: {e}");
                return;
            }
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("insert block create error: {e}");
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            &[]
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            window.set_show_block_type_picker(false);
            // Open the insert window to configure the newly created block
            drop(session_borrow);
            let draft = InsertDraft {
                chain_index,
                block_index: before_index,
                // #716 (model A): a fresh insert is unbound; the user picks its
                // binding later. See TODO(#716) in insert_wiring.rs.
                io: String::new(),
                send_device_id: None,
                send_channels: Vec::new(),
                send_mode: ChainInputMode::Mono,
                return_device_id: None,
                return_channels: Vec::new(),
                return_mode: ChainInputMode::Mono,
            };
            if let Some(iw) = weak_insert_window.upgrade() {
                refresh_input_devices(&chain_input_device_options);
                refresh_output_devices(&chain_output_device_options);
                replace_channel_options(&insert_send_channels, Vec::new());
                replace_channel_options(&insert_return_channels, Vec::new());
                iw.set_selected_send_device_index(-1);
                iw.set_selected_return_device_index(-1);
                iw.set_selected_send_mode_index(0);
                iw.set_selected_return_mode_index(0);
                iw.set_show_block_controls(true);
                iw.set_block_enabled(true);
                iw.set_status_message("".into());
                *insert_draft.borrow_mut() = Some(draft);
                show_child_window(window.window(), iw.window());
            }
            return;
        }
        let models = block_model_picker_items(block_type.effect_type.as_str(), &instrument);
        let Some(model) = models.first() else {
            return;
        };
        if let Some(draft) = block_editor_draft.borrow_mut().as_mut() {
            draft.effect_type = model.effect_type.to_string();
            draft.model_id = model.model_id.to_string();
        }
        let items = block_model_picker_items(block_type.effect_type.as_str(), &instrument);
        block_model_option_labels.set_vec(block_model_picker_labels(&items));
        block_model_options.set_vec(items.clone());
        filtered_block_model_options.set_vec(items);
        // Seed the new block's knobs from the manifest (output_db #655,
        // noise_gate #675) via the single source in `block_factory`, so the
        // editor shows the pre-configured values instead of bare schema
        // defaults. Falls back to empty params if the model is unknown.
        let seeded = application::block_factory::default_params_for_model(
            &model.effect_type,
            &model.model_id,
        )
        .unwrap_or_default();
        let new_params =
            block_parameter_items_for_model(&model.effect_type, &model.model_id, &seeded);
        let overlays = build_knob_overlays(
            project::catalog::model_knob_layout(&model.effect_type, &model.model_id),
            &new_params,
        );
        block_parameter_items.set_vec(new_params);
        multi_slider_points.set_vec(build_multi_slider_points(
            &model.effect_type,
            &model.model_id,
            &ParameterSet::default(),
        ));
        curve_editor_points.set_vec(build_curve_editor_points(
            &model.effect_type,
            &model.model_id,
            &ParameterSet::default(),
        ));
        let (eq_total, eq_bands) = compute_eq_curves(
            &model.effect_type,
            &model.model_id,
            &ParameterSet::default(),
            eq_viz_sample_rate(&project_runtime),
        );
        eq_band_curves.set_vec(
            eq_bands
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        );
        window.set_eq_total_curve(eq_total.into());
        let drawer_state = block_drawer_state(None, &model.effect_type, Some(&model.model_id));
        window.set_block_drawer_title(drawer_state.title.into());
        window.set_block_drawer_confirm_label(drawer_state.confirm_label.into());
        window.set_block_drawer_edit_mode(false);
        window.set_block_drawer_selected_type_index(index);
        window.set_block_drawer_selected_model_index(0);
        window.set_block_drawer_status_message("".into());
        window.set_show_block_type_picker(false);
        if use_inline_block_editor(&window) {
            window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
            window.set_show_block_drawer(true);
        } else {
            window.set_show_block_drawer(false);
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window
                    .set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                sync_block_editor_window(&window, &block_editor_window);
                show_child_window(window.window(), block_editor_window.window());
            }
        }
    });
}
