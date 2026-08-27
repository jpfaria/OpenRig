//! #903 — the global transport through the bus.
//!
//! The panel's play/stop-all is a `Command` like every other looper action, so
//! a MIDI footswitch and MCP reach it too. These drive it the way those
//! transports do — dispatch, no GUI — and check what the store ends up with.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperAction, LooperCommand};
use domain::ids::ChainId;
use engine::LooperState;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, LooperConfig};
use project::project::Project;

use crate::runtime_lifecycle::attach_runtime_control;
use crate::state::ProjectSession;

fn chain_id() -> ChainId {
    ChainId("chain-903-global".into())
}

/// A stopped rig whose (disabled) chain carries two loopers — nothing here
/// opens a device.
fn session_with(loopers: Vec<LooperConfig>) -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers,
        }],
        midi: None,
    };
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-903-global-tests"),
    )
}

fn wired() -> (
    Rc<RefCell<Option<ProjectRuntimeController>>>,
    ProjectSession,
) {
    let runtime: Rc<RefCell<Option<ProjectRuntimeController>>> = Rc::new(RefCell::new(None));
    let session = session_with(vec![LooperConfig::new(1), LooperConfig::new(2)]);
    attach_runtime_control(
        &runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        &session,
    );
    (runtime, session)
}

fn transport(session: &ProjectSession, action: LooperAction) {
    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
            chain: chain_id(),
            looper: 0,
            action,
        }))
        .expect("the transport command is not an error");
}

fn state(runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>, uid: u64) -> Option<LooperState> {
    runtime
        .borrow()
        .as_ref()?
        .chain_looper_status(&chain_id(), uid)
        .map(|s| s.state)
}

/// Two takes loaded, both stopped — the state the panel shows before a global
/// play.
fn load_two_takes(runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>) {
    let borrow = runtime.borrow();
    let controller = borrow.as_ref().expect("the runtime is up");
    for uid in [1u64, 2] {
        controller.looper_load(&chain_id(), uid, &vec![0.3f32; 4_800 * 2]);
    }
}

#[test]
fn play_all_over_the_bus_starts_every_loop_on_the_chain() {
    let (runtime, session) = wired();
    // PlayAll is a request to hear something, so it may bring the runtime up
    // (#808) — that is what creates the store the takes load into.
    transport(&session, LooperAction::PlayAll);
    load_two_takes(&runtime);

    transport(&session, LooperAction::PlayAll);

    assert_eq!(state(&runtime, 1), Some(LooperState::Playing));
    assert_eq!(state(&runtime, 2), Some(LooperState::Playing));
}

#[test]
fn stop_all_over_the_bus_stops_every_loop_on_the_chain() {
    let (runtime, session) = wired();
    transport(&session, LooperAction::PlayAll);
    load_two_takes(&runtime);
    transport(&session, LooperAction::PlayAll);

    transport(&session, LooperAction::StopAll);

    assert_eq!(state(&runtime, 1), Some(LooperState::Stopped));
    assert_eq!(state(&runtime, 2), Some(LooperState::Stopped));
}

/// A row's play still moves ONE loop — the two scopes live side by side.
#[test]
fn a_single_play_over_the_bus_leaves_the_other_loop_alone() {
    let (runtime, session) = wired();
    transport(&session, LooperAction::PlayAll);
    load_two_takes(&runtime);

    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
            chain: chain_id(),
            looper: 1,
            action: LooperAction::Play,
        }))
        .expect("play is not an error");

    assert_eq!(state(&runtime, 1), Some(LooperState::Playing));
    assert_eq!(state(&runtime, 2), Some(LooperState::Stopped));
}

/// #808 stays true for the global scope: a stop never opens a device.
#[test]
fn stop_all_does_not_start_the_audio_runtime() {
    let runtime: Rc<RefCell<Option<ProjectRuntimeController>>> = Rc::new(RefCell::new(None));
    let session = session_with(vec![LooperConfig::new(1)]);
    attach_runtime_control(
        &runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        &session,
    );

    transport(&session, LooperAction::StopAll);

    assert!(
        runtime.borrow().is_none(),
        "silencing is never a reason to open the machine's audio devices"
    );
}
