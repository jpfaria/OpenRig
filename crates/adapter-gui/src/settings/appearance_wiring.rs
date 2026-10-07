//! Responsibility: wires the Settings → Appearance choice.
//!
//! #398: a pick repaints every window at once (the caller hands in the
//! fan-out) and is kept by `appearance_pick`. A scheme set over MCP/gRPC
//! comes back as an event and runs the same repaint (`appearance_events`).
//! What is left here is callback registration and window setters.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global};

use infra_filesystem::{AppConfig, Appearance};

use super::appearance::{appearance_for_index, index_for_appearance};
use super::appearance_pick::{mirror, record};
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

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
        mirror(&app_config, appearance);
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
