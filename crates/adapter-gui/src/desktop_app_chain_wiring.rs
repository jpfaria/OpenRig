//! Chain-level callback wirings dispatched from `run_desktop_app`.
//!
//! Six `*_wiring::wire(...)` / `*_callbacks::wire(...)` calls live here:
//! Chain CRUD, the compact chain view entry, chain name edit, the
//! main-window I/O orchestration, and the per-side input/output groups
//! callbacks. Pulled out of `desktop_app.rs` to land that file under the
//! 600-line cap. Same `&deps` pattern as `desktop_app_block_wiring` —
//! callbacks clone the `Rc` handles they need at registration time.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::{SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::state::{BlockEditorDraft, ChainDraft, IoBlockInsertDraft, ProjectSession};
use crate::{
    AppWindow, ChainEditorWindow, ChainInputGroupsWindow, ChainInputWindow,
    ChainOutputGroupsWindow, ChainOutputWindow, ChannelOptionItem, CompactChainViewWindow,
    ProjectChainItem,
};

#[allow(dead_code)]
pub(crate) struct ChainWiringDeps<'a> {
    pub window: &'a AppWindow,
    pub chain_input_window: &'a ChainInputWindow,
    pub chain_output_window: &'a ChainOutputWindow,
    pub chain_input_groups_window: &'a ChainInputGroupsWindow,
    pub chain_output_groups_window: &'a ChainOutputGroupsWindow,

    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
    pub inline_io_groups_is_input: Rc<Cell<bool>>,

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

    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>>,
    pub vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    pub toast_timer: Rc<Timer>,

    pub vst3_sample_rate: f64,
    pub fullscreen: bool,
    pub auto_save: bool,
}

pub(crate) fn wire_all(deps: &ChainWiringDeps<'_>) {
    // --- Chain CRUD callbacks (extracted to chain_crud_wiring) ---
    crate::chain_crud_wiring::wire(
        &deps.window,
        &deps.chain_input_window,
        &deps.chain_output_window,
        crate::chain_crud_wiring::ChainCrudCtx {
            project_session: deps.project_session.clone(),
            chain_draft: deps.chain_draft.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            chain_input_channels: deps.chain_input_channels.clone(),
            chain_output_channels: deps.chain_output_channels.clone(),
            chain_editor_window: deps.chain_editor_window.clone(),
            chain_input_device_options: deps.chain_input_device_options.clone(),
            chain_output_device_options: deps.chain_output_device_options.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            io_block_insert_draft: deps.io_block_insert_draft.clone(),
            toast_timer: deps.toast_timer.clone(),
            auto_save: deps.auto_save,
            fullscreen: deps.fullscreen,
        },
    );
    // --- on_open_compact_chain_view (extracted to compact_chain_callbacks) ---
    crate::compact_chain_callbacks::wire(
        &deps.window,
        crate::compact_chain_callbacks::CompactChainCallbacksCtx {
            project_session: deps.project_session.clone(),
            project_runtime: deps.project_runtime.clone(),
            project_chains: deps.project_chains.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            toast_timer: deps.toast_timer.clone(),
            open_compact_window: deps.open_compact_window.clone(),
            vst3_editor_handles: deps.vst3_editor_handles.clone(),
            block_editor_draft: deps.block_editor_draft.clone(),
            fullscreen: deps.fullscreen,
            auto_save: deps.auto_save,
            vst3_sample_rate: deps.vst3_sample_rate,
        },
    );
    // --- Chain name edit callback (extracted to chain_name_wiring) ---
    crate::chain_name_wiring::wire(&deps.window, deps.chain_draft.clone());
    // --- Chain I/O main-window callbacks (extracted to chain_io_main_wiring) ---
    crate::chain_io_main_wiring::wire(
        &deps.window,
        &deps.chain_input_window,
        &deps.chain_output_window,
        &deps.chain_input_groups_window,
        &deps.chain_output_groups_window,
        crate::chain_io_main_wiring::ChainIoMainCtx {
            chain_draft: deps.chain_draft.clone(),
            project_session: deps.project_session.clone(),
            chain_editor_window: deps.chain_editor_window.clone(),
            chain_input_device_options: deps.chain_input_device_options.clone(),
            chain_output_device_options: deps.chain_output_device_options.clone(),
            chain_input_channels: deps.chain_input_channels.clone(),
            chain_output_channels: deps.chain_output_channels.clone(),
            inline_io_groups_is_input: deps.inline_io_groups_is_input.clone(),
            toast_timer: deps.toast_timer.clone(),
        },
    );
    // --- ChainInputGroupsWindow callbacks (extracted to chain_input_groups_wiring) ---
    crate::chain_input_groups_wiring::wire(
        &deps.window,
        &deps.chain_input_window,
        &deps.chain_input_groups_window,
        crate::chain_input_groups_wiring::ChainInputGroupsCtx {
            chain_draft: deps.chain_draft.clone(),
            project_session: deps.project_session.clone(),
            chain_input_device_options: deps.chain_input_device_options.clone(),
            chain_output_device_options: deps.chain_output_device_options.clone(),
            chain_input_channels: deps.chain_input_channels.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            auto_save: deps.auto_save,
        },
    );
    // --- ChainOutputGroupsWindow callbacks (extracted to chain_output_groups_wiring) ---
    crate::chain_output_groups_wiring::wire(
        &deps.window,
        &deps.chain_output_window,
        &deps.chain_output_groups_window,
        crate::chain_output_groups_wiring::ChainOutputGroupsCtx {
            chain_draft: deps.chain_draft.clone(),
            project_session: deps.project_session.clone(),
            chain_input_device_options: deps.chain_input_device_options.clone(),
            chain_output_device_options: deps.chain_output_device_options.clone(),
            chain_output_channels: deps.chain_output_channels.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            auto_save: deps.auto_save,
        },
    );
}
