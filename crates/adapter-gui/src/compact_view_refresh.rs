//! Responsibility: re-projects the open compact chain view's block list from the project

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ModelRc, VecModel, Weak};

use crate::compact_block_view::build_compact_blocks;
use crate::state::ProjectSession;
use crate::CompactChainViewWindow;

/// The chain index + handle of the compact view currently on screen, if any.
pub(crate) type OpenCompactWindow = Rc<RefCell<Option<(usize, Weak<CompactChainViewWindow>)>>>;

/// Rebuild the open compact view's `compact_blocks` from the live project.
///
/// Every path that adds, removes or edits a block has to call this: the
/// command mutates the project, but the compact window renders its own model
/// and is not observing it (#898, same class as #667/#614).
pub(crate) fn refresh_open_compact_view(
    open_compact_window: &OpenCompactWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let Some((chain_index, weak)) = open_compact_window
        .borrow()
        .as_ref()
        .map(|(ci, weak)| (*ci, weak.clone()))
    else {
        return;
    };
    let Some(compact_win) = weak.upgrade() else {
        return;
    };
    let session_borrow = project_session.borrow();
    let Some(session) = session_borrow.as_ref() else {
        return;
    };
    let blocks = build_compact_blocks(
        &session.project.borrow(),
        chain_index,
        &session.io_bindings.borrow(),
    );
    compact_win.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
}
