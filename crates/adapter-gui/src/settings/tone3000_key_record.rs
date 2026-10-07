//! Responsibility: records the TONE3000 Secret Key the user saved.
//!
//! Mirrors `integrations_toggle`: the shared boot `AppConfig` snapshot moves
//! with the key, or the next wholesale config save puts the old one back; with
//! a project open the key goes through the dispatcher, which persists it and
//! hands it to the TONE3000 browser. Only the launcher, with no dispatcher,
//! leaves the write to the caller (#879).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, Tone3000Command};
use infra_filesystem::AppConfig;

use crate::state::ProjectSession;

/// What saving a key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyRecord {
    /// A key is set now.
    pub configured: bool,
    /// A dispatcher took it; `false` leaves the write to the caller.
    pub on_the_bus: bool,
}

/// The key as stored: trimmed, blank meaning none.
pub(crate) fn normalize_key(key: &str) -> Option<String> {
    Some(key.trim().to_string()).filter(|k| !k.is_empty())
}

pub(crate) fn record_tone3000_key(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    key: &str,
) -> KeyRecord {
    let key = normalize_key(key);
    let configured = key.is_some();
    app_config.borrow_mut().tone3000.api_key = key.clone();
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return KeyRecord {
            configured,
            on_the_bus: false,
        };
    };
    let command = Command::Tone3000(Tone3000Command::SetTone3000ApiKey {
        key: key.unwrap_or_default(),
    });
    if let Err(e) = session.dispatcher.dispatch(command) {
        log::warn!("[tone3000] saving the key failed: {e}");
    }
    KeyRecord {
        configured,
        on_the_bus: true,
    }
}
