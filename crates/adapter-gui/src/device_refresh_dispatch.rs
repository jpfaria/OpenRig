//! Responsibility: records a device refresh on the command bus.
//!
//! Split out of `device_refresh_wiring` (#913). The re-enumeration itself lives
//! in `device_refresh_apply`; this is the other half of #829 — putting the
//! refresh on the bus so every observer sees it, including the transports that
//! did not ask for it.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, SettingsCommand};

use crate::state::ProjectSession;

/// Dispatch `RefreshAudioDevices`, returning whether it reached a dispatcher.
///
/// The launcher has no project and still refreshes: enumeration does not depend
/// on one, so a session-less window is `false`, not an error.
pub(crate) fn dispatch_refresh(project_session: &Rc<RefCell<Option<ProjectSession>>>) -> bool {
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return false;
    };
    if let Err(e) = session
        .dispatcher
        .dispatch(Command::Settings(SettingsCommand::RefreshAudioDevices))
    {
        log::warn!("refresh devices: {e}");
        return false;
    }
    true
}

#[cfg(test)]
#[path = "device_refresh_dispatch_tests.rs"]
mod tests;
