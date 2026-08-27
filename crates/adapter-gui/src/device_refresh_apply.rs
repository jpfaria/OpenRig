//! Responsibility: re-enumerates the audio devices.
//! The device re-enumeration itself, in one place (#829).
//!
//! Re-enumerating after a USB hot-swap used to live inside the two Slint
//! callbacks — so `Command::RefreshAudioDevices` coming from MCP / gRPC
//! had nothing to run. The work moved here; the callbacks and the event
//! drain both call [`refresh_now`], so a refresh looks the same whoever
//! asked for it (the "dispatch alone is dead" trap, #614).
//!
//! Registered once at wiring time and read back through a thread-local:
//! every caller is on the GUI thread by construction (Slint callbacks and
//! the MCP drain timer both run there), and the handles are `Rc`, so they
//! cannot leave it.

use std::cell::RefCell;
use std::rc::Rc;

use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;
use slint::{SharedString, Timer, VecModel, Weak};

use crate::audio_devices::{
    build_project_device_rows, check_bindings_after_refresh, invalidate_device_cache,
    refresh_input_devices, refresh_output_devices,
};
use crate::helpers::set_status_info;
use crate::state::ProjectSession;
use crate::{AppWindow, DeviceSelectionItem, ProjectSettingsWindow};

/// Everything the refresh writes into. Cloned handles only — no window is
/// held strongly, so registering this never keeps a closed window alive.
pub(crate) struct DeviceRefreshHandles {
    pub(crate) window: Weak<AppWindow>,
    pub(crate) settings_window: Weak<ProjectSettingsWindow>,
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub(crate) chain_input_device_options: Rc<VecModel<SharedString>>,
    pub(crate) chain_output_device_options: Rc<VecModel<SharedString>>,
    pub(crate) toast_timer: Rc<Timer>,
    pub(crate) app_config: Rc<RefCell<AppConfig>>,
    pub(crate) input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
}

thread_local! {
    static HANDLES: RefCell<Option<Rc<DeviceRefreshHandles>>> = const { RefCell::new(None) };
}

/// Publish the handles the refresh runs against. Called once, from the
/// device-refresh wiring.
pub(crate) fn register(handles: Rc<DeviceRefreshHandles>) {
    HANDLES.with(|h| *h.borrow_mut() = Some(handles));
}

/// Re-enumerate the audio interfaces and push the result into every model
/// that shows a device. No-op before wiring (or after the window closed).
///
/// `also_status_window` writes the confirmation into the standalone
/// settings window too — its status field is separate from the main
/// window's toast, which is hidden while it is shown.
pub(crate) fn refresh_now(also_status_window: bool) {
    let handles = HANDLES.with(|h| h.borrow().clone());
    let Some(handles) = handles else {
        return;
    };

    invalidate_device_cache();
    let fresh_input = refresh_input_devices(&handles.chain_input_device_options);
    let fresh_output = refresh_output_devices(&handles.chain_output_device_options);
    // #716: keep the shared descriptor caches + I/O binding pickers live.
    *handles.input_chain_devices.borrow_mut() = fresh_input.clone();
    *handles.output_chain_devices.borrow_mut() = fresh_output.clone();

    let window = handles.window.upgrade();
    let settings_window = handles.settings_window.upgrade();
    if let (Some(w), Some(p)) = (window.as_ref(), settings_window.as_ref()) {
        crate::settings::io_bindings::reseed_device_models(w, p, &fresh_input, &fresh_output);
    }

    if let Some(session) = handles.project_session.borrow().as_ref() {
        handles.project_devices.set_vec(build_project_device_rows(
            &fresh_input,
            &fresh_output,
            &session.project.borrow().device_settings,
        ));
    }

    // #716 Task 13: bindings that reference a now-absent device are never
    // dropped — they stay in the registry flagged unresolved, and we log so
    // the user can be warned.
    for status in check_bindings_after_refresh(
        &handles.app_config.borrow().io_bindings,
        &fresh_input,
        &fresh_output,
    ) {
        if status.unresolved {
            log::warn!(
                "device refresh: binding '{}' references an absent device",
                status.binding.id
            );
        }
    }

    let message = rust_i18n::t!("status-devices-refreshed");
    if let Some(window) = window.as_ref() {
        set_status_info(window, &handles.toast_timer, &message);
    }
    if also_status_window {
        if let Some(sw) = settings_window.as_ref() {
            sw.set_status_message(message.to_string().into());
        }
    }
}
