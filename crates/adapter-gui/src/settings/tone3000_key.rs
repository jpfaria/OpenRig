//! Responsibility: wires the TONE3000 Secret Key field of the settings screen.
//!
//! Both settings surfaces show whether a key is set, never the key. Saving
//! goes through `tone3000_key_record`; from the launcher, which has no
//! dispatcher, the key is written to `config.yaml` directly (#879).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global};

use infra_filesystem::{AppConfig, FilesystemStorage};

use super::tone3000_key_record::record_tone3000_key;
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow, SettingsBridge};

pub fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    app_config: Rc<RefCell<AppConfig>>,
) {
    let configured = app_config.borrow().tone3000.api_key.is_some();
    SettingsBridge::get(window).set_tone3000_key_configured(configured);
    SettingsBridge::get(project_settings_window).set_tone3000_key_configured(configured);

    let weak = window.as_weak();
    let weak_settings = project_settings_window.as_weak();
    let handler = Rc::new(move |key: slint::SharedString| {
        let record = record_tone3000_key(&project_session, &app_config, &key);
        if let Some(w) = weak.upgrade() {
            SettingsBridge::get(&w).set_tone3000_key_configured(record.configured);
        }
        if let Some(s) = weak_settings.upgrade() {
            SettingsBridge::get(&s).set_tone3000_key_configured(record.configured);
        }
        if !record.on_the_bus {
            let key = app_config.borrow().tone3000.api_key.clone();
            application::persist_worker::run(move || persist(key));
        }
    });
    let for_app = handler.clone();
    SettingsBridge::get(window).on_set_tone3000_key(move |key| for_app(key));
    SettingsBridge::get(project_settings_window).on_set_tone3000_key(move |key| handler(key));
}

fn persist(key: Option<String>) {
    let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
    config.tone3000.api_key = key;
    if let Err(e) = FilesystemStorage::save_app_config(&config) {
        log::error!("failed to persist the TONE3000 key: {e}");
    }
}
