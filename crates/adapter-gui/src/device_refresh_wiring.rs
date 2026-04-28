//! Wiring for the "refresh devices" callbacks on the main window and the
//! standalone project-settings window.
//!
//! The two callbacks re-enumerate audio interfaces (after a USB hot-swap, for
//! instance) and rebuild the project devices list. They run in the UI thread.
//!
//! On Linux the refresh is user-initiated only (the button) — periodic polling
//! triggered scarlett2_notify freezes on the Orange Pi USB-C OTG port.
//! On macOS/Windows a periodic watcher (`spawn_hotplug_watcher`) calls
//! `device_set_changed()` and runs the same refresh path automatically.

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(not(target_os = "linux"))]
use slint::Timer;
use slint::{ComponentHandle, SharedString, VecModel};

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
    pub toast_timer: Rc<slint::Timer>,
}

/// Re-enumerate audio devices and repopulate every UI model that lists them.
/// Shared by the manual "Refresh devices" button and the periodic hot-plug
/// watcher (non-Linux only).
fn perform_refresh(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_devices: &Rc<VecModel<DeviceSelectionItem>>,
    chain_input_device_options: &Rc<VecModel<SharedString>>,
    chain_output_device_options: &Rc<VecModel<SharedString>>,
) -> bool {
    invalidate_device_cache();
    let fresh_input = refresh_input_devices(chain_input_device_options);
    let fresh_output = refresh_output_devices(chain_output_device_options);
    let session_borrow = project_session.borrow();
    let Some(session) = session_borrow.as_ref() else {
        return false;
    };
    project_devices.set_vec(build_project_device_rows(
        &fresh_input,
        &fresh_output,
        &session.project.device_settings,
    ));
    true
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    ctx: DeviceRefreshCtx,
) {
    // Issue #354: auto-refresh when audio devices change (hot-swap on macOS/
    // Windows). On Linux a periodic /proc/asound/cards probe freezes the
    // Scarlett 4th Gen on the Orange Pi USB-C OTG port — the manual
    // "Refresh devices" button below remains the only path there.
    #[cfg(not(target_os = "linux"))]
    {
        let timer = spawn_hotplug_watcher(window, project_settings_window, &ctx);
        std::mem::forget(timer);
    }

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
            if !perform_refresh(
                &project_session,
                &project_devices,
                &chain_input_device_options,
                &chain_output_device_options,
            ) {
                return;
            }
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_status_info(&window, &toast_timer, "Lista de dispositivos atualizada");
        });
    }
    {
        let project_settings_window_weak = project_settings_window.as_weak();
        let main_window_weak = window.as_weak();
        project_settings_window.on_refresh_devices(move || {
            if !perform_refresh(
                &project_session,
                &project_devices,
                &chain_input_device_options,
                &chain_output_device_options,
            ) {
                return;
            }
            if let Some(window) = main_window_weak.upgrade() {
                set_status_info(&window, &toast_timer, "Lista de dispositivos atualizada");
            }
            // Settings window has its own status field — the main-window toast is
            // hidden when the standalone settings window is shown, so also clear
            // any stale status on the settings window itself.
            if let Some(sw) = project_settings_window_weak.upgrade() {
                sw.set_status_message("Lista de dispositivos atualizada".into());
            }
        });
    }
}

/// Spawn a periodic watcher that runs `device_set_changed()` and refreshes
/// the device list automatically when a hot-plug, unplug, or interface swap
/// is detected. **Non-Linux only** — Linux relies on the manual "Refresh
/// devices" button because periodic `/proc/asound/cards` reads freeze the
/// Scarlett 4th Gen on the Orange Pi USB-C OTG port (issue #331 origin).
///
/// Returns the timer; caller is responsible for keeping it alive (e.g.
/// `std::mem::forget`) so it isn't dropped when the wiring function returns.
#[cfg(not(target_os = "linux"))]
fn spawn_hotplug_watcher(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    ctx: &DeviceRefreshCtx,
) -> Timer {
    let DeviceRefreshCtx {
        project_session,
        project_devices,
        chain_input_device_options,
        chain_output_device_options,
        toast_timer,
    } = ctx;
    let main_window_weak = window.as_weak();
    let settings_window_weak = project_settings_window.as_weak();
    let project_session = project_session.clone();
    let project_devices = project_devices.clone();
    let chain_input_device_options = chain_input_device_options.clone();
    let chain_output_device_options = chain_output_device_options.clone();
    let toast_timer = toast_timer.clone();

    let timer = Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(2),
        move || {
            if !infra_cpal::device_set_changed() {
                return;
            }
            if !perform_refresh(
                &project_session,
                &project_devices,
                &chain_input_device_options,
                &chain_output_device_options,
            ) {
                return;
            }
            if let Some(window) = main_window_weak.upgrade() {
                set_status_info(
                    &window,
                    &toast_timer,
                    "Audio devices changed — list refreshed",
                );
            }
            if let Some(sw) = settings_window_weak.upgrade() {
                sw.set_status_message("Audio devices changed — list refreshed".into());
            }
        },
    );
    timer
}
