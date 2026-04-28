//! All block-related callback wirings dispatched from `run_desktop_app`.
//!
//! Ten `*_wiring::wire(...)` / `*_callbacks::wire(...)` calls live here:
//! select-block dispatch, chain-block CRUD, insert + choose-type pickers,
//! block model search, picker cancel, drawer close/save/delete, parameter
//! handlers, and the VST3 native-editor opener. Pulled out of
//! `desktop_app.rs` to land that file under the 600-line cap.
//!
//! Takes a `BlockWiringDeps` struct of references rather than 40 individual
//! parameters — the deps still own the data on `run_desktop_app`'s stack
//! and live for the whole `window.run()` blocking call, so closures inside
//! the wirings (which clone the `Rc` handles) stay valid.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::state::{
    BlockEditorDraft, BlockWindow, ChainDraft, InsertDraft, IoBlockInsertDraft, ProjectSession,
    SelectedBlock,
};
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem, BlockTypePickerItem,
    ChainInputGroupsWindow, ChainInputWindow, ChainInsertWindow, ChainOutputGroupsWindow,
    ChainOutputWindow, ChannelOptionItem, CompactChainViewWindow, CurveEditorPoint,
    MultiSliderPoint, PluginInfoWindow, ProjectChainItem,
};

#[allow(dead_code)]
pub(crate) struct BlockWiringDeps<'a> {
    pub window: &'a AppWindow,
    pub block_editor_window: &'a BlockEditorWindow,
    pub chain_input_window: &'a ChainInputWindow,
    pub chain_output_window: &'a ChainOutputWindow,
    pub chain_input_groups_window: &'a ChainInputGroupsWindow,
    pub chain_output_groups_window: &'a ChainOutputGroupsWindow,
    pub chain_insert_window: &'a ChainInsertWindow,

    pub selected_block: Rc<RefCell<Option<SelectedBlock>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub insert_draft: Rc<RefCell<Option<InsertDraft>>>,
    pub io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,

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
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    pub chain_output_channels: Rc<VecModel<ChannelOptionItem>>,

    pub insert_send_channels: Rc<VecModel<ChannelOptionItem>>,
    pub insert_return_channels: Rc<VecModel<ChannelOptionItem>>,

    pub open_block_windows: Rc<RefCell<Vec<BlockWindow>>>,
    pub inline_stream_timer: Rc<RefCell<Option<Timer>>>,
    pub open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>>,
    pub toast_timer: Rc<Timer>,
    pub plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>>,
    pub vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    pub vst3_editor_handles_for_on_open: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    pub block_editor_persist_timer: Rc<Timer>,

    pub vst3_sample_rate: f64,
    pub auto_save: bool,
}

pub(crate) fn wire_all(deps: &BlockWiringDeps<'_>) {
    // --- on_select_chain_block (extracted to select_chain_block_callback + block_editor_window_*) ---
    crate::select_chain_block_callback::wire(
        &deps.window,
        &deps.chain_input_groups_window,
        &deps.chain_output_groups_window,
        &deps.chain_insert_window,
        crate::select_chain_block_callback::SelectChainBlockCallbackCtx {
            selected_block: deps.selected_block.clone(),
            block_editor_draft: deps.block_editor_draft.clone(),
            chain_draft: deps.chain_draft.clone(),
            insert_draft: deps.insert_draft.clone(),
            block_type_options: deps.block_type_options.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            chain_input_device_options: deps.chain_input_device_options.clone(),
            chain_output_device_options: deps.chain_output_device_options.clone(),
            insert_send_channels: deps.insert_send_channels.clone(),
            insert_return_channels: deps.insert_return_channels.clone(),
            open_block_windows: deps.open_block_windows.clone(),
            inline_stream_timer: deps.inline_stream_timer.clone(),
            toast_timer: deps.toast_timer.clone(),
            plugin_info_window: deps.plugin_info_window.clone(),
            vst3_editor_handles: deps.vst3_editor_handles.clone(),
            vst3_sample_rate: deps.vst3_sample_rate,
            auto_save: deps.auto_save,
        },
    );
    // --- Chain block CRUD callbacks (extracted to chain_block_crud_wiring) ---
    crate::chain_block_crud_wiring::wire(
        &deps.window,
        &deps.block_editor_window,
        crate::chain_block_crud_wiring::ChainBlockCrudCtx {
            selected_block: deps.selected_block.clone(),
            block_editor_draft: deps.block_editor_draft.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            block_editor_persist_timer: deps.block_editor_persist_timer.clone(),
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            toast_timer: deps.toast_timer.clone(),
            open_block_windows: deps.open_block_windows.clone(),
            auto_save: deps.auto_save,
        },
    );
    // --- on_start_block_insert + on_choose_block_model (extracted to block_insert_callbacks) ---
    crate::block_insert_callbacks::wire(
        &deps.window,
        &deps.block_editor_window,
        crate::block_insert_callbacks::BlockInsertCallbacksCtx {
            selected_block: deps.selected_block.clone(),
            block_editor_draft: deps.block_editor_draft.clone(),
            block_type_options: deps.block_type_options.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            block_editor_persist_timer: deps.block_editor_persist_timer.clone(),
            auto_save: deps.auto_save,
        },
    );
    // --- on_choose_block_type (extracted to block_choose_type_callback) ---
    crate::block_choose_type_callback::wire(
        &deps.window,
        &deps.block_editor_window,
        &deps.chain_input_window,
        &deps.chain_output_window,
        &deps.chain_insert_window,
        crate::block_choose_type_callback::BlockChooseTypeCallbackCtx {
            block_editor_draft: deps.block_editor_draft.clone(),
            chain_draft: deps.chain_draft.clone(),
            io_block_insert_draft: deps.io_block_insert_draft.clone(),
            insert_draft: deps.insert_draft.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            chain_input_device_options: deps.chain_input_device_options.clone(),
            chain_output_device_options: deps.chain_output_device_options.clone(),
            chain_input_channels: deps.chain_input_channels.clone(),
            chain_output_channels: deps.chain_output_channels.clone(),
            insert_send_channels: deps.insert_send_channels.clone(),
            insert_return_channels: deps.insert_return_channels.clone(),
            auto_save: deps.auto_save,
        },
    );
    // --- Block model search callbacks (extracted to block_model_search_wiring) ---
    crate::block_model_search_wiring::wire(
        &deps.window,
        &deps.block_editor_window,
        deps.block_model_options.clone(),
        deps.filtered_block_model_options.clone(),
    );
    // --- Block picker cancel callback (extracted to block_picker_wiring) ---
    crate::block_picker_wiring::wire(
        &deps.window,
        &deps.block_editor_window,
        crate::block_picker_wiring::BlockPickerCtx {
            block_editor_draft: deps.block_editor_draft.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            block_editor_persist_timer: deps.block_editor_persist_timer.clone(),
        },
    );
    // --- Block drawer close (extracted to block_drawer_close_wiring) ---
    crate::block_drawer_close_wiring::wire(
        &deps.window,
        &deps.block_editor_window,
        crate::block_drawer_close_wiring::BlockDrawerCloseCtx {
            selected_block: deps.selected_block.clone(),
            block_editor_draft: deps.block_editor_draft.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            block_editor_persist_timer: deps.block_editor_persist_timer.clone(),
            inline_stream_timer: deps.inline_stream_timer.clone(),
        },
    );
    // --- Block parameter callbacks (extracted to block_parameter_wiring) ---
    crate::block_parameter_wiring::wire(
        &deps.window,
        &deps.block_editor_window,
        crate::block_parameter_wiring::BlockParameterCtx {
            block_editor_draft: deps.block_editor_draft.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            block_model_options: deps.block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            block_editor_persist_timer: deps.block_editor_persist_timer.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            auto_save: deps.auto_save,
        },
    );
    // --- VST3 editor open (extracted to vst3_editor_wiring) ---
    crate::vst3_editor_wiring::wire(
        &deps.window,
        deps.vst3_editor_handles_for_on_open.clone(),
        deps.vst3_sample_rate,
    );
    // --- Block drawer save+delete callbacks (extracted to block_drawer_save_delete_wiring) ---
    crate::block_drawer_save_delete_wiring::wire(
        &deps.window,
        &deps.block_editor_window,
        crate::block_drawer_save_delete_wiring::BlockDrawerSaveDeleteCtx {
            selected_block: deps.selected_block.clone(),
            block_editor_draft: deps.block_editor_draft.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            block_editor_persist_timer: deps.block_editor_persist_timer.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            open_compact_window: deps.open_compact_window.clone(),
            auto_save: deps.auto_save,
        },
    );
    // --- Block delete confirm/cancel callbacks (extracted to block_delete_wiring) ---
    crate::block_delete_wiring::wire(
        &deps.window,
        &deps.block_editor_window,
        crate::block_delete_wiring::BlockDeleteCtx {
            selected_block: deps.selected_block.clone(),
            block_editor_draft: deps.block_editor_draft.clone(),
            block_model_options: deps.block_model_options.clone(),
            filtered_block_model_options: deps.filtered_block_model_options.clone(),
            block_model_option_labels: deps.block_model_option_labels.clone(),
            block_parameter_items: deps.block_parameter_items.clone(),
            multi_slider_points: deps.multi_slider_points.clone(),
            curve_editor_points: deps.curve_editor_points.clone(),
            eq_band_curves: deps.eq_band_curves.clone(),
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            toast_timer: deps.toast_timer.clone(),
            auto_save: deps.auto_save,
        },
    );
}
