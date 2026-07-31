//! Wiring for the "back to launcher" callback on the main window.
//!
//! Hides the standalone settings/chain editor/block editor windows, clears the
//! in-memory session and chain rows, resets dirty state, and routes the UI back
//! to the launcher view.
//!
//! #127: stopping the audio is NOT done here any more. `CloseProject` stops the
//! rig from the dispatcher, so a project closed over MCP/gRPC goes silent too —
//! it used to leave every stream open and sounding, because the teardown lived
//! in this callback.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use infra_cpal::AudioDeviceDescriptor;
use project::project::Project;

use crate::helpers::clear_status;
use crate::project_ops::set_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use application::command::{Command, ProjectCommand};

use crate::{AppWindow, ChainEditorWindow, ProjectChainItem, ProjectSettingsWindow};

pub(crate) struct BackToLauncherCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
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
    ctx: BackToLauncherCtx,
) {
    let BackToLauncherCtx {
        project_session,
        project_chains,
        saved_project_snapshot,
        project_dirty,
        chain_editor_window,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
    } = ctx;

    let weak_window = window.as_weak();
    let project_settings_window = project_settings_window.as_weak();

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
        // #436 E: closing the project is business → ProjectCommand::CloseProject
        // on the shared dispatcher (MCP/MIDI, observable through
        // Event::ProjectClosed) while the session still exists.
        // #127: that command now also STOPS THE RIG through
        // `RuntimeControl::stop_project_runtime`, so this callback no longer
        // tears the audio down itself — dropping the session below is still
        // adapter-side (the SaveProject precedent). Hiding windows is screen
        // logic.
        if let Some(session) = project_session.borrow().as_ref() {
            if let Err(e) = session
                .dispatcher
                .dispatch(Command::Project(ProjectCommand::CloseProject))
            {
                log::warn!("[back-to-launcher] Command::CloseProject falhou: {e}");
            }
        }
        *project_session.borrow_mut() = None;
        *saved_project_snapshot.borrow_mut() = None;
        replace_project_chains(
            &project_chains,
            &Project {
                name: None,
                device_settings: Vec::new(),
                chains: Vec::new(),
                midi: None,
            },
            &input_chain_devices.borrow(),
            &output_chain_devices.borrow(),
            &[],
        );
        clear_status(&window, &toast_timer);
        set_project_dirty(&window, &project_dirty, false);
        window.set_project_title(rust_i18n::t!("default-project-title").as_ref().into());
        window.set_project_name_draft("".into());
        window.set_project_path_label("".into());
        window.set_show_settings(false);
        window.set_show_chain_editor(false);
        window.set_show_project_chains(false);
        window.set_show_project_setup(false);
        window.set_show_project_launcher(true);
    });
}
