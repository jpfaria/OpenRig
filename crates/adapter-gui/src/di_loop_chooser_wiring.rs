//! Wires the `on_di_loop_choose_file` callback from the chain-tile DI loop
//! control (issue #614, Task 7).
//!
//! Separated from `chain_row_wiring` because that module is under a project
//! rule that forbids `rfd::` imports (issue #511 — native dialogs steal focus
//! on macOS and don't fit Orange Pi touch sessions). The file-picker is
//! acceptable here because it mirrors `block_parameter_wiring`'s use of
//! `rfd::FileDialog` for the block parameter file picker.
//!
//! Called from `desktop_app.rs` after `chain_row_wiring::wire`.

use std::cell::RefCell;
use std::rc::Rc;

use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use rfd::FileDialog;
use slint::{ComponentHandle, Timer};

use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::AppWindow;

/// Wire `on_di_loop_choose_file` on `window`.
///
/// When the user selects "Choose file…" from the ComboBox in the chain-tile
/// DI loop popup, this callback opens a synchronous WAV file picker (same
/// thread pattern as `pick_block_parameter_file` in `block_parameter_wiring`)
/// and dispatches `SetChainDiLoopSource { source: DiLoopSource::File(path) }`.
pub(crate) fn wire(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    toast_timer: Rc<Timer>,
) {
    let weak_window = window.as_weak();
    window.on_di_loop_choose_file(move |index| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let chain_id = {
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let proj = session.project.borrow();
            let Some(chain) = proj.chains.get(index as usize) else {
                return;
            };
            chain.id.clone()
        }; // session_borrow + proj dropped here
           // Synchronous native file dialog — acceptable in this module
           // (see module doc). Blocks the Slint event loop until the OS
           // dialog closes, same as pick_block_parameter_file.
        let Some(path) = FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .pick_file()
        else {
            return; // user cancelled
        };
        let source = DiLoopSource::File(path);
        let cmds = crate::di_loop_wiring::di_loop_commands(
            chain_id,
            crate::di_loop_wiring::DiLoopIntent::SelectSource { source },
        );
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            return;
        };
        for cmd in cmds {
            if let Err(err) = session.dispatcher.dispatch(cmd) {
                set_status_error(&window, &toast_timer, &err.to_string());
                return;
            }
        }
    });
}
