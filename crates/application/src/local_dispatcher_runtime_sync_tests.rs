//! #127 (Task 8), red-first: a chain-runtime sync is a request that goes
//! THROUGH the dispatcher.
//!
//! Before this, a UI callback called `sync_live_chain_runtime` on the audio
//! controller itself right after dispatching. Nothing on the bus could ask for
//! it, so a frontend that is not the GUI had no way to bring a chain's runtime
//! back in step — and the runtime call sat in a Slint closure where no test
//! could reach it.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use project::project::Project;

use crate::command::{ChainCommand, Command};
use crate::dispatcher::CommandDispatcher;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

struct SpyRuntimeControl {
    synced: Rc<RefCell<Vec<String>>>,
}

impl RuntimeControl for SpyRuntimeControl {
    fn sync_chain(&self, chain: &ChainId) -> anyhow::Result<()> {
        self.synced.borrow_mut().push(chain.0.clone());
        Ok(())
    }
}

struct FailingRuntimeControl;

impl RuntimeControl for FailingRuntimeControl {
    fn sync_chain(&self, _chain: &ChainId) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("chain 'x' has no input blocks configured"))
    }
}

fn empty_project() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }))
}

fn sync(chain: &str) -> Command {
    Command::Chain(ChainCommand::SyncChainRuntime {
        chain: ChainId(chain.to_string()),
    })
}

#[test]
fn sync_chain_runtime_reaches_the_attached_runtime() {
    let synced = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        synced: Rc::clone(&synced),
    }));

    let events = dispatcher
        .dispatch(sync("rig:input-3"))
        .expect("SyncChainRuntime must succeed");

    assert_eq!(
        *synced.borrow(),
        vec!["rig:input-3".to_string()],
        "the runtime must be synced by the dispatcher, not by the UI afterwards"
    );
    assert!(
        events.is_empty(),
        "a sync request writes no project state, so it emits no event — an event \
         here would make the frontend's drain schedule a second sync: {events:?}"
    );
}

/// Isolation is a `CLAUDE.md` LAW: syncing one chain touches exactly that
/// chain's runtime, never a group and never "everything at this rate".
#[test]
fn syncing_one_chain_never_touches_another() {
    let synced = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        synced: Rc::clone(&synced),
    }));

    dispatcher.dispatch(sync("rig:input-1")).expect("first");
    dispatcher.dispatch(sync("rig:input-4")).expect("second");

    assert_eq!(
        *synced.borrow(),
        vec!["rig:input-1".to_string(), "rig:input-4".to_string()],
        "each request syncs exactly the chain it named, in order"
    );
}

/// With nothing attached the dispatcher cannot sync anything itself, so it must
/// SAY the sync is still owed rather than report a silent success — the same
/// contract `ToggleBlockEnabled` follows. A frontend that has not yet handed
/// its runtime over then still performs the sync from its event drain.
#[test]
fn an_unapplied_sync_request_says_the_sync_is_still_owed() {
    let dispatcher = LocalDispatcher::new(empty_project());

    let events = dispatcher
        .dispatch(sync("rig:input-3"))
        .expect("a transport with no audio runtime must still dispatch");

    assert!(
        events.iter().any(|e| matches!(
            e,
            crate::event::Event::ChainRuntimeSyncNeeded { chain } if chain.0 == "rig:input-3"
        )),
        "a sync nobody could apply must not report silent success: {events:?}"
    );
}

/// The GUI surfaced a failed sync as a status error; that has to keep working
/// now that the dispatcher makes the call.
#[test]
fn a_failing_sync_surfaces_as_a_dispatch_error() {
    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_runtime_control(Rc::new(FailingRuntimeControl));

    let error = dispatcher
        .dispatch(sync("rig:input-3"))
        .expect_err("a runtime failure must not be swallowed");

    assert!(
        error.to_string().contains("no input blocks"),
        "expected the runtime's own error, got {error}"
    );
}
