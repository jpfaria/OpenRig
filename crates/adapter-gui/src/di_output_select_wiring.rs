//! #771: wiring for the DI panel's OUTPUT select — its own module so the
//! (already oversized) chain_row_wiring / compact_chain_callbacks don't grow.
//! Both entry points delegate to `di_loop_wiring::select_chain_di_output`
//! (persist via `Command::SetChainDiLoopOutput` + re-arm while playing).

use std::cell::RefCell;
use std::rc::Rc;

use infra_cpal::ProjectRuntimeController;

use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow};

/// Main window: `di-loop-output-selected(chain-index, output-index)`.
pub(crate) fn wire_main(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    window.on_di_loop_output_selected(move |index, output_index| {
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            return;
        };
        let chain_id = {
            let proj = session.project.borrow();
            let Some(chain) = proj.chains.get(index as usize) else {
                return;
            };
            chain.id.clone()
        };
        crate::di_loop_wiring::select_chain_di_output(
            &project_runtime,
            &session.dispatcher,
            &chain_id,
            &session.io_bindings.borrow(),
            output_index as usize,
        );
    });
}

/// Detached compact window: `di-loop-output-selected(output-index)` for the
/// focused `chain_index`.
pub(crate) fn wire_compact(
    compact_win: &CompactChainViewWindow,
    chain_index: i32,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    compact_win.on_di_loop_output_selected(move |output_index| {
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            return;
        };
        let chain_id = {
            let proj = session.project.borrow();
            let Some(chain) = proj.chains.get(chain_index as usize) else {
                return;
            };
            chain.id.clone()
        };
        crate::di_loop_wiring::select_chain_di_output(
            &project_runtime,
            &session.dispatcher,
            &chain_id,
            &session.io_bindings.borrow(),
            output_index as usize,
        );
    });
}
