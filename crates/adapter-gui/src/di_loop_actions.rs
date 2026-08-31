//! Responsibility: turns a DI-loop row action into commands on the bus.
//!
//! Split out of `di_loop_chooser_wiring` and `chain_row_wiring_actions` (#913).
//! Four buttons on the chain tile — pick a bundled loop, pick a file, play,
//! stop — and every one of them starts the same way: the ROW INDEX has to
//! become the identity of a chain. A stale index must resolve to "nothing to
//! do", never to whatever chain now sits in that slot.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::di_loader::DiLoopSource;
use domain::ids::ChainId;

use crate::di_loop_wiring::{di_loop_commands, DiLoopIntent};
use crate::state::ProjectSession;

/// The chain the tile at `index` belongs to. `None` ⇒ no project, or the row
/// no longer resolves.
pub(crate) fn chain_id_at(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
) -> Option<ChainId> {
    let borrowed = project_session.borrow();
    let session = borrowed.as_ref()?;
    let project = session.project.borrow();
    project.chains.get(index).map(|chain| chain.id.clone())
}

/// Dispatch `intent` for the chain at `index`.
///
/// `Ok(false)` ⇒ nothing to act on, so the caller shows nothing. `Err` carries
/// the message for the toast.
pub(crate) fn apply_di_loop_intent(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
    intent: DiLoopIntent,
) -> Result<bool, String> {
    let Some(chain_id) = chain_id_at(project_session, index) else {
        return Ok(false);
    };
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return Ok(false);
    };
    for command in di_loop_commands(chain_id, intent) {
        session
            .dispatcher
            .dispatch(command)
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// Point the chain's DI loop at a file the user picked.
pub(crate) fn apply_di_loop_file(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
    path: PathBuf,
) -> Result<bool, String> {
    apply_di_loop_intent(
        project_session,
        index,
        DiLoopIntent::SelectSource {
            source: DiLoopSource::File(path),
        },
    )
}

/// Point the chain's DI loop at the source the ComboBox reports.
///
/// The label of an already-loaded File parses to nothing, which is the right
/// answer: the dispatcher already holds it, so re-selecting it is a no-op
/// rather than a reload (#661). The "choose file…" sentinel never reaches here
/// — the ComboBox routes it to the picker.
pub(crate) fn select_di_loop_source(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
    source_label: &str,
) -> Result<bool, String> {
    let bundled = crate::di_loop_ui_sources::bundled_di_loop_ids();
    let bundled_refs: Vec<&str> = bundled.iter().map(|s| s.as_str()).collect();
    let Some(source) = crate::di_loop_ui_sources::parse_di_loop_source(source_label, &bundled_refs)
    else {
        return Ok(false);
    };
    apply_di_loop_intent(
        project_session,
        index,
        DiLoopIntent::SelectSource { source },
    )
}

/// ▶ — arm this chain's isolated DI stream.
pub(crate) fn play_di_loop(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
) -> Result<bool, String> {
    apply_di_loop_intent(project_session, index, DiLoopIntent::Play)
}

/// ■ — disarm it.
pub(crate) fn stop_di_loop(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
) -> Result<bool, String> {
    apply_di_loop_intent(project_session, index, DiLoopIntent::Stop)
}

#[cfg(test)]
#[path = "di_loop_actions_tests.rs"]
mod tests;
