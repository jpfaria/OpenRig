//! Wiring for the "back to launcher" callback on the main window.
//!
//! Stops the running project runtime, hides the standalone settings/chain
//! editor/block editor windows, clears the in-memory session and chain rows,
//! resets dirty state, and routes the UI back to the launcher view.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::project::Project;

use crate::helpers::clear_status;
use crate::project_ops::set_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::stop_project_runtime;
use crate::{
    AppWindow, BlockEditorWindow, ChainEditorWindow, ProjectChainItem, ProjectSettingsWindow,
};

pub(crate) struct BackToLauncherCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: BackToLauncherCtx,
) {
    let BackToLauncherCtx {
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        chain_editor_window,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
    } = ctx;

    let weak_window = window.as_weak();
    let project_settings_window = project_settings_window.as_weak();
    let block_editor_window = block_editor_window.as_weak();

    window.on_back_to_launcher(move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        if let Some(settings_window) = project_settings_window.upgrade() {
            let _ = settings_window.hide();
        }
        if let Some(editor_window) = chain_editor_window.borrow().as_ref() {
            let _ = editor_window.hide();
        }
        if let Some(editor_window) = block_editor_window.upgrade() {
            let _ = editor_window.hide();
        }
        stop_project_runtime(&project_runtime);
        *project_session.borrow_mut() = None;
        *saved_project_snapshot.borrow_mut() = None;
        replace_project_chains(
            &project_chains,
            &Project {
                name: None,
                device_settings: Vec::new(),
                chains: Vec::new(),
            },
            &input_chain_devices.borrow(),
            &output_chain_devices.borrow(),
        );
        clear_status(&window, &toast_timer);
        set_project_dirty(&window, &project_dirty, false);
        window.set_project_title("Projeto".into());
        window.set_project_name_draft("".into());
        window.set_project_path_label("".into());
        window.set_show_project_settings(false);
        window.set_show_chain_editor(false);
        window.set_show_project_chains(false);
        window.set_show_project_setup(false);
        window.set_show_project_launcher(true);
    });
}
