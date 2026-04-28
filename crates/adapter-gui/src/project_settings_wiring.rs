//! Wiring for project-settings management callbacks (open / name edit / close).
//!
//! Owns the 5 callbacks driving project settings on the main window and the
//! standalone `ProjectSettingsWindow`:
//!
//! - `on_configure_project` — opens settings; refreshes devices first so a
//!   newly connected interface shows up immediately (also handles the
//!   fullscreen inline render path).
//! - Two `on_update_project_name` callbacks — name edits from either window
//!   are mirrored to both windows and write back through to the session.
//! - Two `on_close_project_settings` callbacks — restore the chains view and
//!   clear any toast.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, VecModel};

use infra_cpal::invalidate_device_cache;

use crate::audio_devices::{
    build_project_device_rows, refresh_input_devices, refresh_output_devices,
};
use crate::helpers::{clear_status, set_status_error, show_child_window};
use crate::project_ops::sync_project_dirty;
use crate::state::{AudioSettingsMode, ProjectSession};
use crate::{AppWindow, DeviceSelectionItem, ProjectSettingsWindow};

pub(crate) struct ProjectSettingsCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub audio_settings_mode: Rc<RefCell<AudioSettingsMode>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
    pub fullscreen: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    ctx: ProjectSettingsCtx,
) {
    let ProjectSettingsCtx {
        project_session,
        project_devices,
        chain_input_device_options,
        chain_output_device_options,
        audio_settings_mode,
        saved_project_snapshot,
        project_dirty,
        toast_timer,
        auto_save,
        fullscreen,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_devices = project_devices.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        let project_settings_window = project_settings_window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_configure_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = project_settings_window.upgrade() else {
                return;
            };
            // Invalidate device cache so newly connected/disconnected interfaces appear.
            invalidate_device_cache();
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            project_devices.set_vec(build_project_device_rows(
                &fresh_input,
                &fresh_output,
                &session.project.device_settings,
            ));
            *audio_settings_mode.borrow_mut() = AudioSettingsMode::Project;
            window.set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
            settings_window.set_project_name_draft(
                session.project.name.clone().unwrap_or_default().into(),
            );
            settings_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            if fullscreen {
                // In fullscreen mode, render inline — set project-devices on main window
                window.set_project_devices(settings_window.get_project_devices());
                window.set_show_project_settings(true);
            } else {
                show_child_window(window.window(), settings_window.window());
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_update_project_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_project_name_draft(value.clone());
            if let Some(session) = project_session.borrow_mut().as_mut() {
                let trimmed = value.trim();
                session.project.name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                sync_project_dirty(
                    &window,
                    session,
                    &saved_project_snapshot,
                    &project_dirty,
                    auto_save,
                );
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        let project_session = project_session.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        project_settings_window.on_update_project_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
            window.set_project_name_draft(value.clone());
            settings_window.set_project_name_draft(value.clone());
            if let Some(session) = project_session.borrow_mut().as_mut() {
                let trimmed = value.trim();
                session.project.name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                sync_project_dirty(
                    &window,
                    session,
                    &saved_project_snapshot,
                    &project_dirty,
                    auto_save,
                );
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_close_project_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_show_project_settings(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        project_settings_window.on_close_project_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
            settings_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_project_settings(false);
            let _ = settings_window.hide();
        });
    }
}
