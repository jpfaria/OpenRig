//! #127 (Task 13), red-first: a looper command's RUNTIME half belongs to the
//! dispatcher, not to the GUI callback that dispatched it.
//!
//! A loop lives in the frontend's looper store. Before this, every
//! `LooperCommand` only recorded its project half and emitted an event, and
//! the GUI's `looper_callbacks::dispatch_and_apply` poked the controller's
//! store right afterwards — so an MCP client or a MIDI footswitch pressing
//! RECORD flipped nothing, captured nothing, and the loop stayed `Empty`.
//! (The drain had a second, parallel road for the same reason: two roads to
//! one runtime, which is the split #127 exists to close.)
//!
//! Parity has teeth here: with these doors wired, an MCP client issuing
//! RECORD really does start recording audio.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::LoopPcm;
use project::chain::{Chain, EndpointRef, LooperConfig, LooperSpeed};
use project::project::Project;

use crate::command::{Command, LooperAction, LooperCommand, LooperParam};
use crate::dispatcher::CommandDispatcher;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

/// What the runtime was asked to do, in order. Every entry names the chain it
/// addressed, because isolation is a `CLAUDE.md` LAW: a door that cannot say
/// WHICH stream it touched cannot be proven to have left the others alone.
#[derive(Default)]
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<String>>>,
    /// Loops the store "holds", so the export door can answer like a hosted
    /// runtime does. `None` ⇒ no store hosted at all.
    exports: Option<Vec<(u64, Vec<f32>, u32)>>,
}

impl RuntimeControl for SpyRuntimeControl {
    fn create_looper(&self, chain: &Chain, looper: u64) -> anyhow::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("create {} #{looper}", chain.id.0));
        Ok(())
    }

    fn remove_looper(&self, chain: &Chain, looper: u64) {
        self.calls
            .borrow_mut()
            .push(format!("remove {} #{looper}", chain.id.0));
    }

    fn looper_transport(
        &self,
        chain: &Chain,
        looper: u64,
        action: LooperAction,
    ) -> anyhow::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("{action:?} {} #{looper}", chain.id.0));
        Ok(())
    }

    fn set_looper_param(&self, chain: &Chain, looper: u64, param: LooperParam) {
        self.calls
            .borrow_mut()
            .push(format!("param {param:?} {} #{looper}", chain.id.0));
    }

    fn set_looper_input(&self, chain: &Chain, looper: u64, input: Option<EndpointRef>) {
        self.calls.borrow_mut().push(format!(
            "input {} {} #{looper}",
            input.map(|e| e.endpoint).unwrap_or_else(|| "-".into()),
            chain.id.0
        ));
    }

    fn set_looper_output(&self, chain: &Chain, looper: u64, output: Option<EndpointRef>) {
        self.calls.borrow_mut().push(format!(
            "output {} {} #{looper}",
            output.map(|e| e.endpoint).unwrap_or_else(|| "-".into()),
            chain.id.0
        ));
    }

    fn export_chain_loops(&self, chain: &Chain) -> Option<Vec<(u64, Arc<LoopPcm>)>> {
        self.calls
            .borrow_mut()
            .push(format!("export {}", chain.id.0));
        self.exports.as_ref().map(|loops| {
            loops
                .iter()
                .map(|(uid, pcm, rate)| (*uid, Arc::new(LoopPcm::new(pcm.clone(), *rate))))
                .collect()
        })
    }
}

struct FailingRuntimeControl;

impl RuntimeControl for FailingRuntimeControl {
    fn looper_transport(
        &self,
        _chain: &Chain,
        _looper: u64,
        _action: LooperAction,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("the audio host refused to open"))
    }
}

const UID: u64 = 1;

fn chain_with_loopers(id: &str, uids: &[u64]) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: uids.iter().map(|uid| LooperConfig::new(*uid)).collect(),
    }
}

fn dispatcher_with(chains: Vec<Chain>) -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains,
        midi: None,
    })))
}

/// A dispatcher with one chain holding one looper, and a spy watching its
/// runtime. Returns the call log so a test can assert on what the audio was
/// asked to do — never on the event, which was never the broken half.
fn dispatcher_with_spy(chains: Vec<Chain>) -> (LocalDispatcher, Rc<RefCell<Vec<String>>>) {
    let dispatcher = dispatcher_with(chains);
    let calls = Rc::new(RefCell::new(Vec::new()));
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
        exports: None,
    }));
    (dispatcher, calls)
}

#[test]
fn adding_a_looper_claims_its_slot_in_the_store() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain_with_loopers("rig:input-1", &[])]);
    dispatcher
        .dispatch(Command::Looper(LooperCommand::AddChainLooper {
            chain: ChainId("rig:input-1".into()),
        }))
        .expect("add");
    assert_eq!(
        calls.borrow().as_slice(),
        ["create rig:input-1 #1"],
        "the dispatcher must claim the store slot itself — otherwise a looper \
         added over MCP exists in the project and nowhere the audio can see"
    );
}

#[test]
fn removing_a_looper_frees_its_slot_and_silences_it() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain_with_loopers("rig:input-1", &[UID])]);
    dispatcher
        .dispatch(Command::Looper(LooperCommand::RemoveChainLooper {
            chain: ChainId("rig:input-1".into()),
            looper: UID,
        }))
        .expect("remove");
    assert_eq!(
        calls.borrow().as_slice(),
        ["remove rig:input-1 #1"],
        "deleting a loop must stop its sound from the bus, not only from the \
         GUI's own callback"
    );
}

#[test]
fn every_transport_action_reaches_the_store_through_the_bus() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain_with_loopers("rig:input-1", &[UID])]);
    for action in [
        LooperAction::Record,
        LooperAction::Stop,
        LooperAction::Play,
        LooperAction::PlayStop,
        LooperAction::Undo,
        LooperAction::Redo,
        LooperAction::Clear,
    ] {
        dispatcher
            .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
                chain: ChainId("rig:input-1".into()),
                looper: UID,
                action,
            }))
            .expect("transport");
    }
    assert_eq!(
        calls.borrow().as_slice(),
        [
            "Record rig:input-1 #1",
            "Stop rig:input-1 #1",
            "Play rig:input-1 #1",
            "PlayStop rig:input-1 #1",
            "Undo rig:input-1 #1",
            "Redo rig:input-1 #1",
            "Clear rig:input-1 #1",
        ],
        "a footswitch or an MCP client pressing RECORD must record — the \
         command carries the whole action to the runtime, PlayStop included \
         (the store's own state resolves it, so one button behaves the same \
         on every transport)"
    );
}

#[test]
fn the_footswitch_sentinel_addresses_the_chains_first_looper() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain_with_loopers("rig:input-1", &[7, 9])]);
    dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
            chain: ChainId("rig:input-1".into()),
            looper: 0,
            action: LooperAction::Record,
        }))
        .expect("transport");
    assert_eq!(
        calls.borrow().as_slice(),
        ["Record rig:input-1 #7"],
        "uid 0 is the pedal's sentinel — the runtime must be told the looper \
         it resolved to, never the sentinel"
    );
}

#[test]
fn a_param_edit_reaches_the_live_loop() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain_with_loopers("rig:input-1", &[UID])]);
    for param in [
        LooperParam::Mix(0.25),
        LooperParam::Decay(0.5),
        LooperParam::Speed(LooperSpeed::Double),
        LooperParam::Reverse(true),
    ] {
        dispatcher
            .dispatch(Command::Looper(LooperCommand::SetChainLooperParam {
                chain: ChainId("rig:input-1".into()),
                looper: UID,
                param,
            }))
            .expect("param");
    }
    assert_eq!(
        calls.borrow().as_slice(),
        [
            "param Mix(0.25) rig:input-1 #1",
            "param Decay(0.5) rig:input-1 #1",
            "param Speed(Double) rig:input-1 #1",
            "param Reverse(true) rig:input-1 #1",
        ],
        "a mix/decay/speed/reverse edit must reach the sounding loop from any \
         transport, not only from the on-screen knob"
    );
}

#[test]
fn picking_the_record_input_and_the_play_output_reaches_the_store() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain_with_loopers("rig:input-1", &[UID])]);
    dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperInput {
            chain: ChainId("rig:input-1".into()),
            looper: UID,
            input: Some(EndpointRef {
                binding_id: "io".into(),
                endpoint: "in0".into(),
            }),
        }))
        .expect("input");
    dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperOutput {
            chain: ChainId("rig:input-1".into()),
            looper: UID,
            output: Some(EndpointRef {
                binding_id: "io".into(),
                endpoint: "out1".into(),
            }),
        }))
        .expect("output");
    assert_eq!(
        calls.borrow().as_slice(),
        ["input in0 rig:input-1 #1", "output out1 rig:input-1 #1"],
        "the chosen endpoints must reach the store, or the next REC captures \
         the old input and the loop plays to the old output"
    );
}

/// Isolation (`CLAUDE.md` LAW): a looper command names ONE chain, and no
/// other chain's runtime may be addressed — not even one at the same rate.
#[test]
fn a_looper_command_never_touches_another_chain() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![
        chain_with_loopers("rig:input-1", &[UID]),
        chain_with_loopers("rig:input-2", &[UID]),
    ]);
    dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
            chain: ChainId("rig:input-2".into()),
            looper: UID,
            action: LooperAction::Record,
        }))
        .expect("transport");
    assert_eq!(
        calls.borrow().as_slice(),
        ["Record rig:input-2 #1"],
        "only the named chain's looper may be touched"
    );
}

#[test]
fn a_transport_the_runtime_refused_surfaces_as_a_dispatch_error() {
    let dispatcher = dispatcher_with(vec![chain_with_loopers("rig:input-1", &[UID])]);
    dispatcher.attach_runtime_control(Rc::new(FailingRuntimeControl));
    let result = dispatcher.dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
        chain: ChainId("rig:input-1".into()),
        looper: UID,
        action: LooperAction::Record,
    }));
    assert!(
        result.is_err(),
        "a runtime that could not record must not be swallowed: {result:?}"
    );
}

/// The chain the door is handed is the chain AFTER the project half ran, so
/// the frontend reconciles the loop's stream against what is true now — a
/// removed looper is already gone from the list that decides which streams
/// stay armed.
#[test]
fn the_door_sees_the_chain_as_the_command_left_it() {
    let dispatcher = dispatcher_with(vec![chain_with_loopers("rig:input-1", &[UID])]);
    let seen = Rc::new(RefCell::new(Vec::new()));

    struct ShapeSpy {
        seen: Rc<RefCell<Vec<usize>>>,
    }
    impl RuntimeControl for ShapeSpy {
        fn remove_looper(&self, chain: &Chain, _looper: u64) {
            self.seen.borrow_mut().push(chain.loopers.len());
        }
    }
    dispatcher.attach_runtime_control(Rc::new(ShapeSpy {
        seen: Rc::clone(&seen),
    }));
    dispatcher
        .dispatch(Command::Looper(LooperCommand::RemoveChainLooper {
            chain: ChainId("rig:input-1".into()),
            looper: UID,
        }))
        .expect("remove");
    assert_eq!(
        seen.borrow().as_slice(),
        [0usize],
        "the door must see the chain without the looper the command removed"
    );
}
