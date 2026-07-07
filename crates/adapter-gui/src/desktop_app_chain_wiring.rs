//! Chain-level callback wirings dispatched from `run_desktop_app`.
//!
//! Three `*_wiring::wire(...)` / `*_callbacks::wire(...)` calls live here:
//! Chain CRUD, the compact chain view entry, and chain name edit. Pulled out
//! of `desktop_app.rs` to land that file under the 600-line cap. Same `&deps`
//! pattern as `desktop_app_block_wiring` — callbacks clone the `Rc` handles
//! they need at registration time.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::AppConfig;

use crate::state::{BlockEditorDraft, ChainDraft, ProjectSession};
use crate::{AppWindow, ChainEditorWindow, CompactChainViewWindow, ProjectChainItem};

#[allow(dead_code)]
pub(crate) struct ChainWiringDeps<'a> {
    pub window: &'a AppWindow,

    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,

    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,

    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,

    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>>,
    pub vst3_editor_handles: Rc<RefCell<project::vst3_editor::Vst3EditorRegistry>>,
    pub toast_timer: Rc<Timer>,

    pub app_config: Rc<RefCell<AppConfig>>,
    pub vst3_sample_rate: f64,
    pub fullscreen: bool,
    pub auto_save: bool,
}

pub(crate) fn wire_all(deps: &ChainWiringDeps<'_>) {
    // --- Chain CRUD callbacks (extracted to chain_crud_wiring) ---
    crate::chain_crud_wiring::wire(
        &deps.window,
        crate::chain_crud_wiring::ChainCrudCtx {
            project_session: deps.project_session.clone(),
            chain_draft: deps.chain_draft.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            chain_editor_window: deps.chain_editor_window.clone(),
            project_chains: deps.project_chains.clone(),
            project_runtime: deps.project_runtime.clone(),
            saved_project_snapshot: deps.saved_project_snapshot.clone(),
            project_dirty: deps.project_dirty.clone(),
            toast_timer: deps.toast_timer.clone(),
            app_config: deps.app_config.clone(),
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
            block_editor_draft: deps.block_editor_draft.clone(),
            fullscreen: deps.fullscreen,
            auto_save: deps.auto_save,
        },
    );
    // --- Chain name edit callback (extracted to chain_name_wiring) ---
    crate::chain_name_wiring::wire(&deps.window, deps.chain_draft.clone());
}
