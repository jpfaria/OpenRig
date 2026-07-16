//! #614/#771 — DI loop callbacks for the compact chain window.
//!
//! Extracted verbatim from `compact_chain_callbacks::wire` (which had grown
//! past its line cap) into a focused module. The 4 callbacks target the focused
//! chain (`chain_index`) and delegate to the same helpers the chains-screen
//! tile uses — no duplicate dispatch path.

use std::cell::RefCell;
use std::rc::Rc;

use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use infra_cpal::ProjectRuntimeController;
use slint::{ComponentHandle, Weak};

use crate::compact_chain_callbacks::{compact_chain_di_loop_play, compact_chain_di_loop_stop};
use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow};

/// Wire the compact window's DI-loop callbacks (source select, output select,
/// choose-file, play, stop) for `chain_index`.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    main_weak: Weak<AppWindow>,
    toast_timer: Rc<slint::Timer>,
    chain_index: i32,
) {
    // on_di_loop_source_selected: user picked a bundled source.
    {
        let project_session = project_session.clone();
        let weak_window = main_weak.clone();
        let toast_timer = toast_timer.clone();
        compact_win.on_di_loop_source_selected(move |source_str| {
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
            let source = DiLoopSource::Bundled(source_str.to_string());
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
                    if let Some(main_win) = weak_window.upgrade() {
                        set_status_error(&main_win, &toast_timer, &err.to_string());
                    }
                    return;
                }
            }
        });
    }

    // #771 on_di_loop_output_selected: user picked an output endpoint.
    crate::di_output_select_wiring::wire_compact(
        compact_win,
        chain_index,
        project_session.clone(),
        project_runtime.clone(),
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
        let project_runtime = project_runtime.clone();
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
            compact_chain_di_loop_play(&project_runtime, &session.dispatcher, &chain_id);
        });
    }

    // on_di_loop_stop: user pressed ■ in the compact view.
    {
        let project_session = project_session;
        let project_runtime = project_runtime;
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
            compact_chain_di_loop_stop(&project_runtime, &session.dispatcher, &chain_id);
        });
    }
}
