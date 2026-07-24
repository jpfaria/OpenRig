//! `on_select_chain` — dispatch entry for tapping a chain (not a block) on
//! the Chains screen (issue #591).
//!
//! Selecting a chain makes it the footswitch's active chain: the MIDI slot
//! `toggle_active_chain_enabled` reads `SelectionState.active_chain`, which
//! the daemon mirrors from the dispatcher. Before this, `active_chain` only
//! moved when a *block* was selected (or via MIDI prev/next), so a footswitch
//! stayed frozen on the last block-selected chain regardless of what the user
//! had in front of them.
//!
//! Wired once from `run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer};

use application::dispatcher::CommandDispatcher;

use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::AppWindow;

pub(crate) fn wire(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    toast_timer: Rc<Timer>,
) {
    let weak_window = window.as_weak();
    window.on_select_chain(move |chain_index| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            set_status_error(
                &window,
                &toast_timer,
                &rust_i18n::t!("error-no-project-loaded"),
            );
            return;
        };
        let chain_id = {
            let proj = session.project.borrow();
            match proj.chains.get(chain_index as usize) {
                Some(c) => c.id.clone(),
                None => {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                }
            }
        };
        match session
            .dispatcher
            .dispatch(application::command::Command::Selection(
                application::command::SelectionCommand::SelectActiveChain { chain: chain_id },
            ))
        {
            Ok(_) => {
                // Reflect the selection markers from the dispatcher-owned
                // SelectionState (single source of truth, shared with MIDI).
                let proj = session.project.borrow();
                let sel_arc = session.dispatcher.selection_state();
                let sel = sel_arc.read().expect("selection state poisoned");
                crate::selection_highlight::sync_selection_markers(&window, &proj, &sel);
            }
            Err(_) => {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
            }
        }
    });
}
