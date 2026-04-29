//! Wiring for the "refresh devices" callbacks on the main window and the
//! standalone project-settings window.
//!
//! The two callbacks re-enumerate audio interfaces (after a USB hot-swap, for
//! instance) and rebuild the project devices list. They run in the UI thread
//! and are rate-limited by user clicks — no periodic polling, since that
//! triggered scarlett2_notify freezes on the Orange Pi USB-C OTG port.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, VecModel};

use infra_cpal::invalidate_device_cache;

use crate::audio_devices::{
    build_project_device_rows, refresh_input_devices, refresh_output_devices,
};
use crate::helpers::set_status_info;
use crate::state::ProjectSession;
use crate::{AppWindow, DeviceSelectionItem, ProjectSettingsWindow};

pub(crate) struct DeviceRefreshCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub toast_timer: Rc<Timer>,
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    ctx: DeviceRefreshCtx,
) {
    let DeviceRefreshCtx {
        project_session,
        project_devices,
        chain_input_device_options,
        chain_output_device_options,
        toast_timer,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_devices = project_devices.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let toast_timer = toast_timer.clone();
        window.on_refresh_devices(move || {
            invalidate_device_cache();
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            project_devices.set_vec(build_project_device_rows(
                &fresh_input,
                &fresh_output,
                &session.project.device_settings,
            ));
            set_status_info(&window, &toast_timer, &rust_i18n::t!("Lista de dispositivos atualizada"));
        });
    }
    {
        let project_settings_window_weak = project_settings_window.as_weak();
        let main_window_weak = window.as_weak();
        project_settings_window.on_refresh_devices(move || {
            invalidate_device_cache();
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            project_devices.set_vec(build_project_device_rows(
                &fresh_input,
                &fresh_output,
                &session.project.device_settings,
            ));
            if let Some(window) = main_window_weak.upgrade() {
                set_status_info(&window, &toast_timer, &rust_i18n::t!("Lista de dispositivos atualizada"));
            }
            // Settings window has its own status field — the main-window toast is
            // hidden when the standalone settings window is shown, so also clear
            // any stale status on the settings window itself.
            if let Some(sw) = project_settings_window_weak.upgrade() {
                sw.set_status_message(rust_i18n::t!("Lista de dispositivos atualizada").to_string().into());
            }
        });
    }
}
