//! Responsibility: wires the Settings → Appearance choice.
//!
//! #398: System / Light / Dark. A pick repaints every window at once (the
//! caller hands in the fan-out) and is kept in `config.yaml`: through the bus
//! when a project is open, so MCP reaches the same command, or directly from
//! the launcher, which has no dispatcher. The boot `AppConfig` snapshot is
//! mirrored so a wholesale re-save (recent projects, project open) never
//! brings the old scheme back. A scheme set over MCP/gRPC comes back as an
//! event and runs the same repaint (`appearance_events`).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global};

use application::command::{Command, SettingsCommand};
use infra_filesystem::{AppConfig, Appearance, FilesystemStorage};

use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow, ThemeMode};

/// The segment order of the section: System, Light, Dark.
pub(crate) fn appearance_for_index(index: i32) -> Appearance {
    match index {
        1 => Appearance::Light,
        2 => Appearance::Dark,
        _ => Appearance::System,
    }
}

pub(crate) fn index_for_appearance(appearance: Appearance) -> i32 {
    match appearance {
        Appearance::System => 0,
        Appearance::Light => 1,
        Appearance::Dark => 2,
    }
}

pub(crate) fn theme_mode_for(appearance: Appearance) -> ThemeMode {
    match appearance {
        Appearance::System => ThemeMode::System,
        Appearance::Light => ThemeMode::Light,
        Appearance::Dark => ThemeMode::Dark,
    }
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    app_config: Rc<RefCell<AppConfig>>,
    paint_all: impl Fn(Appearance) + 'static,
) {
    let stored = app_config.borrow().appearance;
    show(window, project_settings_window, stored);
    paint_all(stored);

    let weak = window.as_weak();
    let weak_settings = project_settings_window.as_weak();
    let reflect: Rc<dyn Fn(Appearance)> = Rc::new(move |appearance| {
        if let (Some(w), Some(s)) = (weak.upgrade(), weak_settings.upgrade()) {
            show(&w, &s, appearance);
        }
        paint_all(appearance);
        app_config.borrow_mut().appearance = appearance;
    });
    // A scheme set over MCP/gRPC repaints like a pick here.
    crate::appearance_events::install(reflect.clone());
    let handler = Rc::new(move |index: i32| {
        let appearance = appearance_for_index(index);
        reflect(appearance);
        record(&project_session, appearance);
    });
    let for_app = handler.clone();
    crate::SettingsBridge::get(window).on_appearance_selected(move |i| for_app(i));
    crate::SettingsBridge::get(project_settings_window).on_appearance_selected(move |i| handler(i));
}

/// Lights the chosen segment on both settings surfaces.
fn show(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    appearance: Appearance,
) {
    let index = index_for_appearance(appearance);
    crate::SettingsBridge::get(window).set_appearance_index(index);
    crate::SettingsBridge::get(project_settings_window).set_appearance_index(index);
}

/// On the bus when a project is open; straight to `config.yaml` otherwise.
fn record(project_session: &Rc<RefCell<Option<ProjectSession>>>, appearance: Appearance) {
    if let Some(session) = project_session.borrow().as_ref() {
        let command = Command::Settings(SettingsCommand::SetAppearance { appearance });
        if let Err(e) = session.dispatcher.dispatch(command) {
            log::warn!("[appearance] dispatch failed: {e}");
        }
        return;
    }
    application::persist_worker::run(move || {
        if let Err(e) = FilesystemStorage::save_gui_appearance(appearance) {
            log::error!("failed to persist appearance: {e}");
        }
    });
}
