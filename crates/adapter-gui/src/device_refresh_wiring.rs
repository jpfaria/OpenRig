//! Wiring for the "refresh devices" callbacks on the main window and the
//! standalone project-settings window.
//!
//! The two callbacks re-enumerate audio interfaces (after a USB hot-swap, for
//! instance) and rebuild the project devices list. They run in the UI thread
//! and are rate-limited by user clicks — no periodic polling, since that
//! triggered scarlett2_notify freezes on the Orange Pi USB-C OTG port.
//!
//! ## Hot-swap binding resolution (#716, Task 13)
//!
//! After re-enumerating, `check_bindings_after_refresh` is called to mark any
//! I/O binding that references a now-absent device as unresolved. Bindings are
//! NEVER silently dropped — they stay in the registry with `unresolved = true`
//! so the UI can surface a warning to the user.

use std::cell::RefCell;
use std::rc::Rc;

use infra_cpal::invalidate_device_cache;
use infra_filesystem::AppConfig;
use slint::{ComponentHandle, SharedString, Timer, VecModel};

use crate::audio_devices::{
    build_project_device_rows, check_bindings_after_refresh, refresh_input_devices,
    refresh_output_devices,
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
    /// Shared in-memory app config — inspected (not mutated) to detect
    /// bindings that become unresolved after a hot-swap refresh.
    pub app_config: Rc<RefCell<AppConfig>>,
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
        app_config,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_devices = project_devices.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let toast_timer = toast_timer.clone();
        let app_config = app_config.clone();
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
                &session.project.borrow().device_settings,
            ));
            // #716 Task 13: log any bindings that became unresolved after the
            // hot-swap. Bindings are never dropped — they stay in the registry
            // with `unresolved = true` so the UI can surface a warning.
            for status in check_bindings_after_refresh(
                &app_config.borrow().io_bindings,
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
            set_status_info(
                &window,
                &toast_timer,
                &rust_i18n::t!("status-devices-refreshed"),
            );
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
                &session.project.borrow().device_settings,
            ));
            // #716 Task 13: same unresolved-binding detection on the standalone
            // settings window refresh path.
            for status in check_bindings_after_refresh(
                &app_config.borrow().io_bindings,
                &fresh_input,
                &fresh_output,
            ) {
                if status.unresolved {
                    log::warn!(
                        "device refresh (settings): binding '{}' references an absent device",
                        status.binding.id
                    );
                }
            }
            if let Some(window) = main_window_weak.upgrade() {
                set_status_info(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("status-devices-refreshed"),
                );
            }
            // Settings window has its own status field — the main-window toast is
            // hidden when the standalone settings window is shown, so also clear
            // any stale status on the settings window itself.
            if let Some(sw) = project_settings_window_weak.upgrade() {
                sw.set_status_message(rust_i18n::t!("status-devices-refreshed").to_string().into());
            }
        });
    }
}
