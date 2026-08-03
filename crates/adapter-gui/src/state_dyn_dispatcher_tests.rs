//! #127: the GUI session must accept any `CommandDispatcher`, not only the
//! in-process `LocalDispatcher`. The recording double below is deliberately
//! unrelated to the engine — if it can back a `ProjectSession`, the frontend
//! is decoupled from the local implementation.

use super::*;
use application::command::{Command, ProjectCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use std::cell::RefCell as StdRefCell;

/// Records what the UI dispatched, proving the session is not bound to
/// `LocalDispatcher`.
struct RecordingDispatcher {
    seen: StdRefCell<Vec<String>>,
    selection: std::sync::Arc<std::sync::RwLock<application::selection_state::SelectionState>>,
}

impl CommandDispatcher for RecordingDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(format!("{cmd:?}"));
        Ok(Vec::new())
    }
    fn selection_state(
        &self,
    ) -> std::sync::Arc<std::sync::RwLock<application::selection_state::SelectionState>> {
        std::sync::Arc::clone(&self.selection)
    }
}

#[test]
fn session_accepts_a_non_local_dispatcher() {
    let recorder = std::rc::Rc::new(RecordingDispatcher {
        seen: StdRefCell::new(Vec::new()),
        selection: std::sync::Arc::new(std::sync::RwLock::new(Default::default())),
    });
    let session = ProjectSession::with_dispatcher(
        Project::default(),
        std::rc::Rc::clone(&recorder) as std::rc::Rc<dyn CommandDispatcher>,
        None,
        None,
        std::path::PathBuf::from("presets"),
    );

    session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("dispatch");

    assert_eq!(recorder.seen.borrow().len(), 1);
}
