//! Orchestrator that wires every callback registered on the per-instance
//! `ChainEditorWindow`.
//!
//! Delegates to two sibling modules:
//!
//! - `chain_editor_meta_io_callbacks` — chain name + instrument editing.
//! - `chain_editor_save_cancel_callbacks` — save (validate + commit + sync
//!   live runtime), cancel, and the #716 I/O binding checklist toggle.
//!
//! Called once per editor instance from `chain_crud_wiring::wire` (which
//! creates a fresh `ChainEditorWindow` on add/configure).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::state::{ChainDraft, ProjectSession};
use crate::{AppWindow, ChainEditorWindow, ProjectChainItem};

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
    toast_timer: Rc<Timer>,
    auto_save: bool,
) {
    crate::chain_editor_meta_io_callbacks::wire(
        editor_window,
        weak_window.clone(),
        chain_draft.clone(),
    );

    crate::chain_editor_save_cancel_callbacks::wire(
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
        toast_timer,
        auto_save,
    );
}
