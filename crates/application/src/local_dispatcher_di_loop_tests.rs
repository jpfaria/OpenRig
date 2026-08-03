//! #127 (Task 12), red-first: arming a chain's DI is a runtime effect the
//! DISPATCHER applies, not something the GUI does after dispatching.
//!
//! Before this, `SetChainDiLoopEnabled` only emitted its event and the GUI's
//! own callback poked `ProjectRuntimeController::arm_di_stream` right
//! afterwards — so the same command over MCP/gRPC changed nothing audible, and
//! the arm call sat in a Slint closure no test could reach. The DI is a
//! user-visible state change, so it goes through the bus like every other one.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::DiPcm;
use project::chain::{Chain, DiOutputRef};
use project::project::Project;

use crate::command::{ChainCommand, Command};
use crate::di_loader::DiLoopSource;
use crate::dispatcher::CommandDispatcher;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

/// What the runtime was asked to do, in order. The chain id is part of every
/// entry because isolation is a `CLAUDE.md` LAW: a door that cannot say WHICH
/// stream it addressed cannot be proven to have left the others alone.
#[derive(Default)]
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<String>>>,
}

impl RuntimeControl for SpyRuntimeControl {
    fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> anyhow::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("arm {} @{}", chain.id.0, pcm.src_sr()));
        Ok(())
    }

    fn disarm_di_stream(&self, chain: &ChainId) {
        self.calls.borrow_mut().push(format!("disarm {}", chain.0));
    }

    fn refresh_di_stream(&self, chain: &Chain, _pcm: Arc<DiPcm>) -> anyhow::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("refresh {}", chain.id.0));
        Ok(())
    }
}

struct FailingRuntimeControl;

impl RuntimeControl for FailingRuntimeControl {
    fn arm_di_stream(&self, _chain: &Chain, _pcm: Arc<DiPcm>) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("no output endpoint for the DI"))
    }
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn project_with(chains: Vec<Chain>) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains,
        midi: None,
    }))
}

/// A dispatcher holding a decoded source for each named chain, exactly as
/// `SetChainDiLoopSource`'s decode task installs it.
fn dispatcher_with_sources(ids: &[&str]) -> LocalDispatcher {
    let dispatcher = LocalDispatcher::new(project_with(ids.iter().map(|id| chain(id)).collect()));
    for id in ids {
        dispatcher.di_loop_state.borrow_mut().insert(
            ChainId(id.to_string()),
            (
                DiLoopSource::Bundled((*id).to_string()),
                Arc::new(DiPcm::new(vec![0.0; 8], 44_100, 1)),
            ),
        );
    }
    dispatcher
}

fn set_enabled(chain: &str, enabled: bool) -> Command {
    Command::Chain(ChainCommand::SetChainDiLoopEnabled {
        chain: ChainId(chain.to_string()),
        enabled,
    })
}

#[test]
fn playing_the_di_arms_that_chains_stream_through_the_bus() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher_with_sources(&["rig:input-1"]);
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));

    dispatcher
        .dispatch(set_enabled("rig:input-1", true))
        .expect("SetChainDiLoopEnabled must succeed");

    assert_eq!(
        *calls.borrow(),
        vec!["arm rig:input-1 @44100".to_string()],
        "the dispatcher must arm the chain's DI stream with the loaded source — \
         otherwise the same command over MCP/gRPC plays nothing"
    );
}

#[test]
fn stopping_the_di_disarms_that_chains_stream_through_the_bus() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher_with_sources(&["rig:input-1"]);
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));

    dispatcher
        .dispatch(set_enabled("rig:input-1", false))
        .expect("SetChainDiLoopEnabled must succeed");

    assert_eq!(
        *calls.borrow(),
        vec!["disarm rig:input-1".to_string()],
        "stopping the DI must reach the runtime from the bus"
    );
}

/// Isolation (`CLAUDE.md` LAW): arming one chain's DI addresses exactly that
/// chain. Never a group, never "every chain at this rate".
#[test]
fn arming_one_chains_di_never_touches_another() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher_with_sources(&["rig:input-1", "rig:input-2"]);
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));

    dispatcher
        .dispatch(set_enabled("rig:input-2", true))
        .expect("play");
    dispatcher
        .dispatch(set_enabled("rig:input-2", false))
        .expect("stop");

    assert_eq!(
        *calls.borrow(),
        vec![
            "arm rig:input-2 @44100".to_string(),
            "disarm rig:input-2".to_string()
        ],
        "only the named chain's stream may be touched"
    );
}

/// The UI may only offer Play once a source is loaded, but MCP can ask for it
/// at any time. With nothing decoded there is nothing to play, and the honest
/// effect is the one the GUI already applied: clear the stream rather than arm
/// silence.
#[test]
fn playing_with_no_source_loaded_disarms_instead_of_arming() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = LocalDispatcher::new(project_with(vec![chain("rig:input-1")]));
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));

    dispatcher
        .dispatch(set_enabled("rig:input-1", true))
        .expect("a play with no source is not an error");

    assert_eq!(
        *calls.borrow(),
        vec!["disarm rig:input-1".to_string()],
        "with no decoded source the stream must be cleared, never armed empty"
    );
}

/// #771: picking the DI's output endpoint moves a PLAYING loop to it. The
/// re-arm is the runtime's business, so the command carries it — and a stream
/// that is not playing must not be started by an output pick.
#[test]
fn picking_the_di_output_refreshes_that_chains_stream() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher_with_sources(&["rig:input-1"]);
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));

    dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopOutput {
            chain: ChainId("rig:input-1".into()),
            output: DiOutputRef {
                binding_id: "io-1".into(),
                endpoint: "out-L".into(),
            },
        }))
        .expect("SetChainDiLoopOutput must succeed");

    assert_eq!(
        *calls.borrow(),
        vec!["refresh rig:input-1".to_string()],
        "the output pick must ask the runtime to follow it, not leave the sound \
         on the old endpoint until the next play"
    );
}

/// A runtime that cannot arm (no reachable output endpoint) must say so. The
/// GUI already swallows its own play failures; a transport that asks for the
/// arm deserves the reason.
#[test]
fn a_failing_arm_surfaces_as_a_dispatch_error() {
    let dispatcher = dispatcher_with_sources(&["rig:input-1"]);
    dispatcher.attach_runtime_control(Rc::new(FailingRuntimeControl));

    let error = dispatcher
        .dispatch(set_enabled("rig:input-1", true))
        .expect_err("a runtime failure must not be swallowed");

    assert!(
        error.to_string().contains("no output endpoint"),
        "expected the runtime's own error, got {error}"
    );
}

/// #669: the device rate changed under a loop that is PLAYING. The loop was
/// resampled to the OLD rate, so it drags in slow motion until it is re-armed
/// against the rebuilt runtime. The dispatcher owns the rate, so it owns the
/// refresh — the GUI used to run this loop itself, which left a rate change
/// driven from anywhere else (an MCP-triggered re-sync) dragging.
#[test]
fn a_device_rate_change_refreshes_the_loops_that_are_playing() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher_with_sources(&["rig:input-1"]);
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));

    let flagged = dispatcher.attach_engine_sr(44_100);

    assert_eq!(
        flagged,
        vec![ChainId("rig:input-1".into())],
        "the rate change must still report the chains with a loaded source"
    );
    assert_eq!(
        *calls.borrow(),
        vec!["refresh rig:input-1".to_string()],
        "a loop playing at the old rate must be re-armed at the new one"
    );
}

/// The same rate is not a change: nothing is re-armed, so a re-sync that
/// resolves the rate it already had never interrupts a playing loop.
#[test]
fn re_publishing_the_same_rate_touches_no_stream() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher_with_sources(&["rig:input-1"]);
    dispatcher.attach_engine_sr(44_100);
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));

    dispatcher.attach_engine_sr(44_100);

    assert!(
        calls.borrow().is_empty(),
        "an unchanged rate must not re-arm anything: {:?}",
        calls.borrow()
    );
}

/// A transport with no audio runtime attached still dispatches: the event is
/// reported and nothing is applied.
#[test]
fn a_transport_with_no_runtime_still_reports_the_event() {
    let dispatcher = dispatcher_with_sources(&["rig:input-1"]);

    let events = dispatcher
        .dispatch(set_enabled("rig:input-1", true))
        .expect("no attached runtime is not an error");

    assert!(
        events.iter().any(|e| matches!(
            e,
            crate::event::Event::ChainDiLoopEnabledChanged { chain, enabled }
                if chain.0 == "rig:input-1" && *enabled
        )),
        "the state change must still be reported: {events:?}"
    );
}
