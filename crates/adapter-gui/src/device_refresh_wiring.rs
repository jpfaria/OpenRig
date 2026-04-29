//! Wiring for the "refresh devices" callbacks on the main window and the
//! standalone project-settings window.
//!
//! The two callbacks re-enumerate audio interfaces (after a USB hot-swap, for
//! instance) and rebuild the project devices list. They run in the UI thread.
//!
//! Auto-refresh is on by default and uses a different mechanism per platform:
//! - **macOS / Windows**: 2 s polling timer (`spawn_periodic_hotplug_watcher`)
//!   that calls `device_set_changed()` and triggers refresh on transitions.
//! - **Linux**: event-driven `inotify` watcher on `/dev/snd/`
//!   (`infra_cpal::spawn_linux_hotplug_watcher`). Zero polling — the kernel
//!   wakes the watcher thread only on real card add/remove. Periodic
//!   `/proc/asound/cards` reads triggered scarlett2_notify freezes on the
//!   Orange Pi USB-C OTG port (#331), so polling is not an option there.
//!
//! In every case the eventual refresh runs on the UI thread (Slint forbids
//! mutating models from background threads). The Linux watcher posts back
//! via `slint::invoke_from_event_loop`.

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
    // Issue #354: auto-refresh when audio devices change (hot-swap).
    // - macOS/Windows: 2 s polling timer (cheap, host enumeration).
    // - Linux: event-driven inotify on /dev/snd (zero polling, safe for
    //   Scarlett 4th Gen on Orange Pi USB-C OTG — see #331).
    #[cfg(not(target_os = "linux"))]
    {
        let timer = spawn_periodic_hotplug_watcher(window, project_settings_window, &ctx);
        std::mem::forget(timer);
    }
    #[cfg(target_os = "linux")]
    spawn_linux_hotplug_watcher(window, project_settings_window, &ctx);

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

/// Run the device-list refresh after a hot-plug event, then surface a toast
/// on the appropriate window. Pure UI-thread work — caller MUST be on the
/// Slint event loop.
fn refresh_and_toast(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_devices: &Rc<VecModel<DeviceSelectionItem>>,
    chain_input_device_options: &Rc<VecModel<SharedString>>,
    chain_output_device_options: &Rc<VecModel<SharedString>>,
    main_window_weak: &slint::Weak<AppWindow>,
    settings_window_weak: &slint::Weak<ProjectSettingsWindow>,
    toast_timer: &Rc<slint::Timer>,
) {
    if !perform_refresh(
        project_session,
        project_devices,
        chain_input_device_options,
        chain_output_device_options,
    ) {
        return;
    }
    if let Some(window) = main_window_weak.upgrade() {
        set_status_info(
            &window,
            toast_timer,
            "Audio devices changed — list refreshed",
        );
    }
    if let Some(sw) = settings_window_weak.upgrade() {
        sw.set_status_message("Audio devices changed — list refreshed".into());
    }
}

/// Spawn a periodic watcher that runs `device_set_changed()` and refreshes
/// the device list automatically when a hot-plug, unplug, or interface swap
/// is detected. **macOS / Windows only** — Linux uses the event-driven
/// inotify watcher because periodic `/proc/asound/cards` reads freeze the
/// Scarlett 4th Gen on the Orange Pi USB-C OTG port (issue #331 origin).
#[cfg(not(target_os = "linux"))]
fn spawn_periodic_hotplug_watcher(
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
            refresh_and_toast(
                &project_session,
                &project_devices,
                &chain_input_device_options,
                &chain_output_device_options,
                &main_window_weak,
                &settings_window_weak,
                &toast_timer,
            );
        },
    );
    timer
}

/// Spawn the Linux event-driven hotplug watcher. The watcher thread blocks
/// on inotify events for `/dev/snd/`; on each batch it posts back to the
/// Slint event loop so the refresh runs on the UI thread.
///
/// Refresh always runs (no fingerprint gate) — kernel events are already
/// rare and edge-triggered, and a redundant refresh is cheap.
#[cfg(target_os = "linux")]
fn spawn_linux_hotplug_watcher(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    ctx: &DeviceRefreshCtx,
) {
    let DeviceRefreshCtx {
        project_session,
        project_devices,
        chain_input_device_options,
        chain_output_device_options,
        toast_timer,
    } = ctx;
    // Snapshot weak handles + Rcs into the closure that runs on the UI loop.
    // The inotify thread itself only schedules; it captures nothing UI-bound.
    let main_window_weak = window.as_weak();
    let settings_window_weak = project_settings_window.as_weak();
    let project_session = project_session.clone();
    let project_devices = project_devices.clone();
    let chain_input_device_options = chain_input_device_options.clone();
    let chain_output_device_options = chain_output_device_options.clone();
    let toast_timer = toast_timer.clone();

    infra_cpal::spawn_linux_hotplug_watcher(move || {
        // We're on the inotify thread. Hop to the Slint event loop before
        // touching any UI state. Clone everything the closure needs because
        // invoke_from_event_loop runs the closure asynchronously, so the
        // FnMut here is called many times.
        let main_window_weak = main_window_weak.clone();
        let settings_window_weak = settings_window_weak.clone();
        let project_session = project_session.clone();
        let project_devices = project_devices.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let toast_timer = toast_timer.clone();
        if let Err(e) = slint::invoke_from_event_loop(move || {
            refresh_and_toast(
                &project_session,
                &project_devices,
                &chain_input_device_options,
                &chain_output_device_options,
                &main_window_weak,
                &settings_window_weak,
                &toast_timer,
            );
        }) {
            log::warn!("hotplug watcher: cannot post to UI loop: {}", e);
        }
    });
}
