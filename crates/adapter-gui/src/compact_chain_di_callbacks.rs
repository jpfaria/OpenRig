//! Responsibility: handles the DI loop actions of the compact chain view.
//! #614/#771 — DI loop callbacks for the compact chain window.
//!
//! Extracted verbatim from `compact_chain_callbacks::wire` (which had grown
//! past its line cap) into a focused module. The 4 callbacks target the focused
//! chain (`chain_index`) and delegate to the same helpers the chains-screen
//! tile uses — no duplicate dispatch path.

use std::cell::RefCell;
use std::rc::Rc;

use application::di_loader::DiLoopSource;
use slint::Weak;

use crate::compact_chain_callbacks::{compact_chain_di_loop_play, compact_chain_di_loop_stop};
use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow};

/// Wire the compact window's DI-loop callbacks (source select, output select,
/// choose-file, play, stop) for `chain_index`.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    main_weak: Weak<AppWindow>,
    toast_timer: Rc<slint::Timer>,
    chain_index: i32,
) {
    // on_di_loop_source_selected: user picked a bundled source.
    {
        let project_session = project_session.clone();
        let weak_window = main_weak.clone();
        let toast_timer = toast_timer.clone();
        // #827: the list carries saved looper takes as well as bundled loops,
        // so the label resolves through the same parser the chain tile uses.
        compact_win.on_di_loop_source_selected(move |source_str| {
            on_source_picked(
                chain_index,
                |row| {
                    crate::di_loop_actions::select_di_loop_source(
                        &project_session,
                        row,
                        &source_str,
                    )
                },
                &weak_window,
                &toast_timer,
            );
        });
    }

    // #771 on_di_loop_output_selected: user picked an output endpoint.
    crate::di_output_select_wiring::wire_compact(compact_win, chain_index, project_session.clone());

    // #1021: the DI panel's trash on a saved take.
    crate::di_take_delete_wiring::wire_compact(
        compact_win,
        &project_session,
        main_weak.clone(),
        toast_timer.clone(),
    );

    // on_di_loop_choose_file: user picked "Choose file…" — open native dialog.
    {
        let project_session = project_session.clone();
        let weak_window = main_weak.clone();
        let toast_timer = toast_timer.clone();
        compact_win.on_di_loop_choose_file(move || {
            let chain_id = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            let Some(path) = rfd::FileDialog::new()
                .add_filter("WAV audio", &["wav"])
                .pick_file()
            else {
                return; // user cancelled
            };
            let cmds = crate::di_loop_wiring::di_loop_commands(
                chain_id,
                crate::di_loop_wiring::DiLoopIntent::SelectSource {
                    source: DiLoopSource::File(path),
                },
            );
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            for cmd in cmds {
                if let Err(err) = session.dispatcher.dispatch(cmd) {
                    if let Some(main_win) = weak_window.upgrade() {
                        set_status_error(&main_win, &toast_timer, &err.to_string());
                    }
                    return;
                }
            }
        });
    }

    // on_di_loop_play: user pressed ▶ in the compact view.
    {
        let project_session = project_session.clone();
        compact_win.on_di_loop_play(move || {
            let chain_id = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            // #808 lives behind the door now: arming an independent pipeline
            // creates the runtime, so play works with no chain enabled.
            compact_chain_di_loop_play(session.dispatcher.as_ref(), &chain_id);
        });
    }

    // on_di_loop_stop: user pressed ■ in the compact view.
    {
        let project_session = project_session;
        compact_win.on_di_loop_stop(move || {
            let chain_id = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            compact_chain_di_loop_stop(session.dispatcher.as_ref(), &chain_id);
        });
    }
}

/// A DI source picked in the compact window: nothing when the window names no
/// chain, else `select` for its row; a refusal becomes the main window's toast.
fn on_source_picked(
    chain_index: i32,
    select: impl FnOnce(usize) -> Result<bool, String>,
    main_weak: &Weak<AppWindow>,
    toast_timer: &slint::Timer,
) {
    if chain_index < 0 {
        return;
    }
    if let Err(err) = select(chain_index as usize) {
        if let Some(main_win) = main_weak.upgrade() {
            set_status_error(&main_win, toast_timer, &err);
        }
    }
}

#[cfg(test)]
#[path = "compact_chain_di_callbacks_tests.rs"]
mod tests;
