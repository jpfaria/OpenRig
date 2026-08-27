//! Responsibility: records an integrations toggle in memory and on the bus.
//!
//! Split out of `settings::integrations` (#913). Painting the switch and the
//! direct-persist fallback are the caller's; this is the part that must not be
//! got wrong twice over:
//!
//! * the shared boot `AppConfig` snapshot is mirrored, or the next wholesale
//!   re-save (recent projects, project open) clobbers the toggle and the switch
//!   resets on restart;
//! * with a project open the change goes through the dispatcher so MCP/gRPC see
//!   it, and only the launcher — which has no dispatcher — falls back to a
//!   direct write.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use infra_filesystem::AppConfig;

use crate::state::ProjectSession;

/// Mirror `enabled` into the shared snapshot and put it on the bus.
///
/// Returns whether a dispatcher took it. `false` ⇒ the launcher: the caller
/// persists directly, because there is no bus to record on.
pub(crate) fn record_toggle(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    set_in_snapshot: impl Fn(&mut AppConfig, bool),
    make_command: impl Fn(bool) -> Command,
    enabled: bool,
) -> bool {
    set_in_snapshot(&mut app_config.borrow_mut(), enabled);
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return false;
    };
    if let Err(e) = session.dispatcher.dispatch(make_command(enabled)) {
        log::warn!("[integrations] subsystem toggle dispatch failed: {e}");
    }
    true
}
