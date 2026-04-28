//! `on_choose_block_type` — branches on the type chosen from the block-type
//! picker.
//!
//! * `insert`: creates an empty `InsertBlock` immediately (so it shows up in
//!   the chain), then opens the insert configuration window with a fresh
//!   `InsertDraft` for the user to fill in send/return endpoints.
//! * `input` / `output`: stores an `IoBlockInsertDraft` (consumed later by
//!   the I/O save flow) and opens the dedicated I/O endpoint editor with a
//!   single empty group seeded into a temporary `ChainDraft`.
//! * effect type (everything else): prefills the block drawer with the first
//!   available model, builds parameter items + knob overlays, and shows the
//!   inline drawer or the detached editor depending on capabilities.
//!
//! Wired once from `run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::AudioBlockKind;
use project::chain::{ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;

use crate::audio_devices::{
    refresh_input_devices, refresh_output_devices, replace_channel_options,
};
use crate::block_editor::{block_parameter_items_for_model, build_knob_overlays};
use crate::eq::{build_curve_editor_points, build_multi_slider_points, compute_eq_curves};
use crate::helpers::{show_child_window, sync_block_editor_window, use_inline_block_editor};
use crate::io_groups::{apply_chain_input_window_state, apply_chain_output_window_state};
use crate::project_ops::sync_project_dirty;
use crate::project_view::{
    block_model_picker_items, block_model_picker_labels, block_type_picker_items,
    replace_project_chains,
};
use crate::state::{
    BlockEditorDraft, ChainDraft, InputGroupDraft, InsertDraft, IoBlockInsertDraft,
    OutputGroupDraft, ProjectSession,
};
use crate::sync_live_chain_runtime;
use crate::ui_state::block_drawer_state;
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem,
    ChainInputWindow, ChainInsertWindow, ChainOutputWindow, ChannelOptionItem,
    CurveEditorPoint, MultiSliderPoint, ProjectChainItem,
};

pub(crate) struct BlockChooseTypeCallbackCtx {
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
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
    pub chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    pub chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
    pub insert_send_channels: Rc<VecModel<ChannelOptionItem>>,
    pub insert_return_channels: Rc<VecModel<ChannelOptionItem>>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    chain_input_window: &ChainInputWindow,
    chain_output_window: &ChainOutputWindow,
    chain_insert_window: &ChainInsertWindow,
    ctx: BlockChooseTypeCallbackCtx,
) {
    let BlockChooseTypeCallbackCtx {
        block_editor_draft,
        chain_draft,
        io_block_insert_draft,
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
        chain_input_channels,
        chain_output_channels,
        insert_send_channels,
        insert_return_channels,
        auto_save,
    } = ctx;

    let weak_window = window.as_weak();
    let weak_block_editor_window = block_editor_window.as_weak();
    let weak_input_window = chain_input_window.as_weak();
    let weak_output_window = chain_output_window.as_weak();
    let weak_insert_window = chain_insert_window.as_weak();

    window.on_choose_block_type(move |index| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let instrument = block_editor_draft.borrow().as_ref()
            .map(|d| d.instrument.clone())
            .unwrap_or_else(|| block_core::DEFAULT_INSTRUMENT.to_string());
        let block_types = block_type_picker_items(&instrument);
        let Some(block_type) = block_types.get(index as usize) else {
            return;
        };
        log::debug!("on_choose_block_type: index={}, type='{}', instrument='{}'", index, block_type.effect_type, instrument);

        // Handle I/O and Insert block types: open the dedicated window instead of the block editor
        let effect_type_str = block_type.effect_type.as_str();
        if effect_type_str == "insert" {
            // Insert block: create directly with empty endpoints
            let (chain_index, before_index) = {
                let draft_borrow = block_editor_draft.borrow();
                let Some(draft) = draft_borrow.as_ref() else { return; };
                (draft.chain_index, draft.before_index)
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else { return; };
            let Some(chain) = session.project.chains.get(chain_index) else { return; };
            let block_id = domain::ids::BlockId(format!("{}:insert:{}", chain.id.0, before_index));
            drop(session_borrow);
            let insert_block = project::block::AudioBlock {
                id: block_id,
                enabled: true,
                kind: AudioBlockKind::Insert(project::block::InsertBlock {
                    model: "standard".to_string(),
                    send: project::block::InsertEndpoint {
                        device_id: domain::ids::DeviceId(String::new()),
                        mode: ChainInputMode::Mono,
                        channels: Vec::new(),
                    },
                    return_: project::block::InsertEndpoint {
                        device_id: domain::ids::DeviceId(String::new()),
                        mode: ChainInputMode::Mono,
                        channels: Vec::new(),
                    },
                }),
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_index) else { return; };
            chain.blocks.insert(before_index, insert_block);
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("insert block create error: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            window.set_show_block_type_picker(false);
            // Open the insert window to configure the newly created block
            drop(session_borrow);
            let draft = InsertDraft {
                chain_index,
                block_index: before_index,
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
        if effect_type_str == "input" || effect_type_str == "output" {
            let (chain_index, before_index) = {
                let draft_borrow = block_editor_draft.borrow();
                let Some(draft) = draft_borrow.as_ref() else { return; };
                (draft.chain_index, draft.before_index)
            };
            // Store the I/O insert draft
            *io_block_insert_draft.borrow_mut() = Some(IoBlockInsertDraft {
                chain_index,
                before_index,
                kind: effect_type_str.to_string(),
            });
            window.set_show_block_type_picker(false);

            if effect_type_str == "input" {
                // Set up a temporary chain draft for the input window callbacks
                let input_group = InputGroupDraft {
                    device_id: None,
                    channels: Vec::new(),
                    mode: ChainInputMode::Mono,
                };
                *chain_draft.borrow_mut() = Some(ChainDraft {
                    editing_index: Some(chain_index),
                    name: String::new(),
                    instrument: instrument.clone(),
                    inputs: vec![input_group.clone()],
                    outputs: Vec::new(),
                    editing_input_index: Some(0),
                    editing_output_index: None,
                    editing_io_block_index: None,
                    adding_new_input: false,
                    adding_new_output: false,
                });
                if let Some(input_window) = weak_input_window.upgrade() {
                    let fresh_input = refresh_input_devices(&chain_input_device_options);
                    let draft_borrow = chain_draft.borrow();
                    let draft = draft_borrow.as_ref().unwrap();
                    if let Some(session) = project_session.borrow().as_ref() {
                        apply_chain_input_window_state(
                            &input_window,
                            &input_group,
                            draft,
                            &session.project,
                            &fresh_input,
                            &chain_input_channels,
                        );
                    }
                    show_child_window(window.window(), input_window.window());
                }
            } else {
                // Set up a temporary chain draft for the output window callbacks
                let output_group = OutputGroupDraft {
                    device_id: None,
                    channels: Vec::new(),
                    mode: ChainOutputMode::Stereo,
                };
                *chain_draft.borrow_mut() = Some(ChainDraft {
                    editing_index: Some(chain_index),
                    name: String::new(),
                    instrument: instrument.clone(),
                    inputs: Vec::new(),
                    outputs: vec![output_group.clone()],
                    editing_io_block_index: None,
                    editing_input_index: None,
                    editing_output_index: Some(0),
                    adding_new_input: false,
                    adding_new_output: false,
                });
                if let Some(output_window) = weak_output_window.upgrade() {
                    let fresh_output = refresh_output_devices(&chain_output_device_options);
                    apply_chain_output_window_state(
                        &output_window,
                        &output_group,
                        &fresh_output,
                        &chain_output_channels,
                    );
                    show_child_window(window.window(), output_window.window());
                }
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
        let drawer_state =
            block_drawer_state(None, &model.effect_type, Some(&model.model_id));
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
                block_editor_window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                sync_block_editor_window(&window, &block_editor_window);
                show_child_window(window.window(), block_editor_window.window());
            }
        }
    });
}
