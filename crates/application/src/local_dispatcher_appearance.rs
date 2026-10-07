//! Responsibility: handles the appearance command.
//! #398 — `SettingsCommand::SetAppearance`: the light/dark scheme is a
//! per-machine preference (ADR 0003). The handler keeps it in `config.yaml`
//! on the persist worker, touching only that field; the GUI repaints its
//! windows itself.

use anyhow::Result;
use infra_filesystem::Appearance;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_set_appearance(&self, appearance: Appearance) -> Result<Vec<Event>> {
        // #731: bind the config path at dispatch time (see app_config_persist).
        crate::app_config_persist::persist_app_config(move |config| config.appearance = appearance);
        Ok(vec![Event::AppearanceChanged { appearance }])
    }
}
