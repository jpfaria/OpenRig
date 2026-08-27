//! Responsibility: points a chain's DI loop at a file the user chose.
//!
//! Split out of `di_loop_chooser_wiring` (#913). Opening the native picker and
//! showing the error is screen work; resolving which chain the row index means
//! and putting the choice on the bus is not — a pick that stops at the GUI
//! leaves the loop playing whatever it had.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::di_loader::DiLoopSource;

use crate::state::ProjectSession;

/// Dispatch the file pick for the chain at `index`.
///
/// `Ok(false)` ⇒ there was nothing to point at (no project, no such chain), so
/// the caller shows nothing. `Err` carries the message to put in the toast.
pub(crate) fn apply_di_loop_file(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
    path: PathBuf,
) -> Result<bool, String> {
    let chain_id = {
        let borrowed = project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return Ok(false);
        };
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(index) else {
            return Ok(false);
        };
        chain.id.clone()
    };
    let commands = crate::di_loop_wiring::di_loop_commands(
        chain_id,
        crate::di_loop_wiring::DiLoopIntent::SelectSource {
            source: DiLoopSource::File(path),
        },
    );
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return Ok(false);
    };
    for command in commands {
        session
            .dispatcher
            .dispatch(command)
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[cfg(test)]
#[path = "di_loop_file_pick_tests.rs"]
mod tests;
