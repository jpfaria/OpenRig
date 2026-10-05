//! #827 — a DI source picked in the compact chain window.
//!
//! The list carries bundled loops and saved looper takes; the pick resolves to
//! this window's chain row. A window that names no chain does nothing, and a
//! refusal reaches the user as the main window's error toast — never silently.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use slint::ComponentHandle;

use super::{on_source_picked, wire};
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow};

#[test]
fn a_window_that_names_no_chain_selects_nothing() {
    i_slint_backend_testing::init_no_event_loop();
    let called = Cell::new(false);
    on_source_picked(
        -1,
        |_| {
            called.set(true);
            Ok(true)
        },
        &slint::Weak::default(),
        &slint::Timer::default(),
    );
    assert!(
        !called.get(),
        "chain index -1 must not reach the dispatcher"
    );
}

#[test]
fn the_pick_goes_to_this_windows_row() {
    i_slint_backend_testing::init_no_event_loop();
    let row = Cell::new(None);
    on_source_picked(
        2,
        |index| {
            row.set(Some(index));
            Ok(true)
        },
        &slint::Weak::default(),
        &slint::Timer::default(),
    );
    assert_eq!(row.get(), Some(2));
}

#[test]
fn a_refused_pick_shows_the_error_on_the_main_window() {
    i_slint_backend_testing::init_no_event_loop();
    let main = AppWindow::new().expect("main window");
    let toast_timer = slint::Timer::default();
    on_source_picked(
        0,
        |_| Err("the take could not be read".to_string()),
        &main.as_weak(),
        &toast_timer,
    );
    assert_eq!(main.get_status_message(), "the take could not be read");
}

#[test]
fn a_refused_pick_after_the_main_window_closed_is_dropped() {
    i_slint_backend_testing::init_no_event_loop();
    // Nothing to show it on; the call must simply return.
    on_source_picked(
        0,
        |_| Err("late".to_string()),
        &slint::Weak::default(),
        &slint::Timer::default(),
    );
}

/// Records every command; accepts them all.
#[derive(Default)]
struct SpyDispatcher {
    seen: RefCell<Vec<Command>>,
    selection: std::sync::Arc<std::sync::RwLock<application::SelectionState>>,
}

impl CommandDispatcher for SpyDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(cmd);
        Ok(vec![])
    }

    fn selection_state(&self) -> std::sync::Arc<std::sync::RwLock<application::SelectionState>> {
        std::sync::Arc::clone(&self.selection)
    }
}

#[test]
fn a_label_that_names_no_source_dispatches_nothing() {
    i_slint_backend_testing::init_no_event_loop();
    let compact = CompactChainViewWindow::new().expect("compact window");
    let main = AppWindow::new().expect("main window");
    let spy = Rc::new(SpyDispatcher::default());
    let session = ProjectSession::with_dispatcher(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![Chain {
                id: ChainId("rig:in".into()),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: false,
                volume: 100.0,
                io_binding_ids: vec![],
                blocks: vec![],
                di_output: None,
                loopers: vec![],
                disabled_endpoints: Default::default(),
                mix: Default::default(),
            }],
            midi: None,
        },
        Rc::clone(&spy) as Rc<dyn CommandDispatcher>,
        None,
        None,
        std::env::temp_dir().join("openrig-827-compact-di-tests"),
    );
    wire(
        &compact,
        Rc::new(RefCell::new(Some(session))),
        main.as_weak(),
        Rc::new(slint::Timer::default()),
        0,
    );

    // #661: the label of a file the dispatcher already holds parses to
    // nothing, so re-picking it must not reload it.
    compact.invoke_di_loop_source_selected("a-file-already-loaded.wav".into());

    assert!(spy.seen.borrow().is_empty());
    assert_eq!(main.get_status_message(), "");
}
