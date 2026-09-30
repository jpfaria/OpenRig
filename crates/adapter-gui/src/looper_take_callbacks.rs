//! Responsibility: wires the waveform editor's Save take button to the bus.
//! #827 — keep the open loop as a named take.
//!
//! Dispatch only: `SaveChainLooperTake` exports the mixdown and writes the wav
//! (the same command an MCP client sends). This module maps the outcome to the
//! status the editor shows, and nothing else.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperCommand};
use application::event::Event;
use application::looper_take_library::TakeSaveError;
use slint::ComponentHandle;

use crate::state::ProjectSession;
use crate::{AppWindow, LooperEditor};

// The editor's `take-status` codes (looper_panel_globals.slint).
pub(crate) const TAKE_SAVED: i32 = 1;
pub(crate) const TAKE_NAME_TAKEN: i32 = 2;
pub(crate) const TAKE_NOTHING_RECORDED: i32 = 3;
pub(crate) const TAKE_FAILED: i32 = 4;
pub(crate) const TAKE_NEEDS_NAME: i32 = 5;

/// The editor's `take-status` for a save's outcome. An error the core did not
/// type is a failed save — never reported as a success.
pub(crate) fn take_status_code(result: &anyhow::Result<Vec<Event>>) -> i32 {
    let Err(err) = result else {
        return TAKE_SAVED;
    };
    match err.downcast_ref::<TakeSaveError>() {
        Some(TakeSaveError::NameTaken(_)) => TAKE_NAME_TAKEN,
        Some(TakeSaveError::NothingRecorded) => TAKE_NOTHING_RECORDED,
        Some(TakeSaveError::EmptyName) => TAKE_NEEDS_NAME,
        Some(TakeSaveError::Io(_)) | None => TAKE_FAILED,
    }
}

type Session = Rc<RefCell<Option<ProjectSession>>>;

/// Wire the editor's `save-take` to `SaveChainLooperTake`. On success the name
/// is cleared; on a refusal it is kept so it can be fixed, not retyped.
pub(crate) fn wire_looper_take_callbacks(window: &AppWindow, session: &Session) {
    let session = session.clone();
    let window_weak = window.as_weak();
    window
        .global::<LooperEditor>()
        .on_save_take(move |index, uid, name| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let session_borrow = session.borrow();
            let Some(s) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = crate::di_loop_actions::chain_id_at(&session, index as usize) else {
                return;
            };
            let result =
                s.dispatcher
                    .dispatch(Command::Looper(LooperCommand::SaveChainLooperTake {
                        chain,
                        looper: uid as u64,
                        name: name.to_string(),
                    }));
            if let Err(err) = &result {
                log::warn!("saving the take was refused: {err}");
            }
            let status = take_status_code(&result);
            let editor = window.global::<LooperEditor>();
            if status == TAKE_SAVED {
                editor.set_take_name(Default::default());
            }
            editor.set_take_status(status);
        });
}

#[cfg(test)]
#[path = "looper_take_callbacks_tests.rs"]
mod tests;
