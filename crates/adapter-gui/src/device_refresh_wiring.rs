//! Wiring for the "refresh devices" callbacks on the main window and the
//! standalone project-settings window.
//!
//! The callbacks are pure dispatchers (#829): each one submits
//! `SettingsCommand::RefreshAudioDevices` and then applies the refresh
//! inline via [`crate::device_refresh_apply`], the same entry point the
//! MCP/gRPC drain runs when it sees `Event::AudioDevicesRefreshed`. The
//! work itself — re-enumeration, model rebuild, binding re-check — lives
//! there, so no transport has a private path.
//!
//! They run in the UI thread and are rate-limited by user clicks — no
//! periodic polling, since that triggered scarlett2_notify freezes on the
//! Orange Pi USB-C OTG port.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, SettingsCommand};
use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;
use slint::{ComponentHandle, Global, SharedString, Timer, VecModel};

use crate::device_refresh_apply::{refresh_now, register, DeviceRefreshHandles};
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
    /// #716: shared descriptor caches updated on refresh so the I/O bindings
    /// editor's device pickers and channel derivation see the live hardware.
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
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
        input_chain_devices,
        output_chain_devices,
    } = ctx;

    register(Rc::new(DeviceRefreshHandles {
        window: window.as_weak(),
        settings_window: project_settings_window.as_weak(),
        project_session: project_session.clone(),
        project_devices,
        chain_input_device_options,
        chain_output_device_options,
        toast_timer,
        app_config,
        input_chain_devices,
        output_chain_devices,
    }));

    {
        let project_session = project_session.clone();
        crate::SettingsBridge::get(window).on_refresh_devices(move || {
            dispatch_refresh(&project_session);
            refresh_now(false);
        });
    }
    crate::SettingsBridge::get(project_settings_window).on_refresh_devices(move || {
        dispatch_refresh(&project_session);
        refresh_now(true);
    });
}

/// Record the refresh on the command bus so every observer sees it. A
/// session-less window (launcher) still refreshes — the enumeration does
/// not depend on a project.
fn dispatch_refresh(project_session: &Rc<RefCell<Option<ProjectSession>>>) {
    if let Some(session) = project_session.borrow().as_ref() {
        if let Err(e) = session
            .dispatcher
            .dispatch(Command::Settings(SettingsCommand::RefreshAudioDevices))
        {
            log::warn!("refresh devices: {e}");
        }
    }
}
