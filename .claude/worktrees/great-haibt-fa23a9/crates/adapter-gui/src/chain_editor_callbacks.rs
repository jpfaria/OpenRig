//! Orchestrator that wires every callback registered on the per-instance
//! `ChainEditorWindow`.
//!
//! Delegates to four sibling modules, each owning a coherent slice of the
//! chain editor surface:
//!
//! - `chain_editor_meta_io_callbacks` — chain name + instrument editing,
//!   input / output group edit / add / remove.
//! - `chain_editor_save_cancel_callbacks` — save (validate + commit + sync
//!   live runtime) and cancel of the whole chain.
//! - `chain_editor_input_endpoint_callbacks` — inline input endpoint editor
//!   (select-device, toggle-channel, select-mode, save, cancel).
//! - `chain_editor_output_endpoint_callbacks` — same for the output side.
//!
//! Called once per editor instance from `chain_crud_wiring::wire` (which
//! creates a fresh `ChainEditorWindow` on add/configure).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::state::{ChainDraft, IoBlockInsertDraft, ProjectSession};
use crate::{
    AppWindow, ChainEditorWindow, ChainInputWindow, ChainOutputWindow, ChannelOptionItem,
    ProjectChainItem,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn setup_chain_editor_callbacks(
    editor_window: &ChainEditorWindow,
    weak_window: slint::Weak<AppWindow>,
    chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    chain_input_device_options: Rc<VecModel<SharedString>>,
    chain_output_device_options: Rc<VecModel<SharedString>>,
    chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
    _weak_input_window: slint::Weak<ChainInputWindow>,
    _weak_output_window: slint::Weak<ChainOutputWindow>,
    io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
    toast_timer: Rc<Timer>,
    auto_save: bool,
) {
    crate::chain_editor_meta_io_callbacks::wire(
        editor_window,
        weak_window.clone(),
        chain_draft.clone(),
        input_chain_devices.clone(),
        output_chain_devices.clone(),
        chain_input_device_options,
        chain_output_device_options,
        chain_input_channels,
        chain_output_channels,
    );

    crate::chain_editor_save_cancel_callbacks::wire(
        editor_window,
        weak_window.clone(),
        chain_draft.clone(),
        project_session.clone(),
        project_chains.clone(),
        project_runtime.clone(),
        saved_project_snapshot.clone(),
        project_dirty.clone(),
        input_chain_devices.clone(),
        output_chain_devices.clone(),
        toast_timer,
        auto_save,
    );

    crate::chain_editor_input_endpoint_callbacks::wire(
        editor_window,
        weak_window.clone(),
        chain_draft.clone(),
        project_session.clone(),
        project_chains.clone(),
        project_runtime.clone(),
        saved_project_snapshot.clone(),
        project_dirty.clone(),
        input_chain_devices.clone(),
        output_chain_devices.clone(),
        io_block_insert_draft.clone(),
        auto_save,
    );

    crate::chain_editor_output_endpoint_callbacks::wire(
        editor_window,
        weak_window,
        chain_draft,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        io_block_insert_draft,
        auto_save,
    );
}
