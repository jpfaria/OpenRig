//! #14 — starting the metronome must NOT require a chain to be enabled.
//!
//! Same root cause the DI hit in #808: the runtime controller is created lazily,
//! only when a chain is enabled (`sync_live_chain_runtime`). A user who opens a
//! project, never enables a chain, and presses POWER on the metronome had no
//! controller at all — so `start_click` opened no stream, no click played, and
//! the beat lamp sat frozen on beat 1. The metronome is an independent pipeline
//! (invariant #4); it must create the runtime the same way the DI does.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

use crate::metronome_wiring::ensure_metronome_runtime;
use crate::state::ProjectSession;

fn session_with_disabled_chain() -> Rc<RefCell<Option<ProjectSession>>> {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("metronome-14-play".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false, // opened the project, never enabled the chain
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    let session = ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-metronome-14-play"),
    );
    Rc::new(RefCell::new(Some(session)))
}

#[test]
fn starting_the_metronome_creates_a_runtime_without_enabling_a_chain() {
    let project_session = session_with_disabled_chain();
    let project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: no chain enabled, so no controller exists yet"
    );

    ensure_metronome_runtime(&project_runtime, &project_session);

    assert!(
        project_runtime.borrow().is_some(),
        "#14: pressing POWER on the metronome must create the runtime controller \
         even with no chain enabled — otherwise the click never opens its stream \
         and the beat lamp freezes on beat one (exactly the reported symptom)."
    );
}

#[test]
fn ensuring_the_runtime_is_a_no_op_with_no_project_open() {
    // No project session (launcher screen): nothing to build a runtime from,
    // and it must not panic.
    let project_session: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    let project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));

    ensure_metronome_runtime(&project_runtime, &project_session);

    assert!(
        project_runtime.borrow().is_none(),
        "with no project open there is nothing to build a runtime from"
    );
}
