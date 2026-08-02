//! Issue #323 / #127 — a looper command reaches the controller's looper store,
//! and it gets there on the BUS.
//!
//! A dispatch alone used to be dead (#614): the dispatcher recorded the
//! project half and the GUI callback pushed the runtime half into the store
//! right afterwards, with the external-event drain running a second copy for
//! MCP/MIDI. Task 13 moved that half onto
//! `application::runtime_control::RuntimeControl`, so these tests dispatch
//! `LooperCommand`s — never a GUI function, never an event applier — and
//! assert on the store and on the isolated playback stream.

mod common;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use application::command::{Command, LooperAction, LooperCommand, LooperParam};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::LooperState;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef, LooperConfig, LooperSpeed};
use project::project::Project;

use common::LooperRuntimeControl;

const UID: u64 = 1;

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![
            IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            },
            IoEndpoint {
                name: "out1".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![2, 3],
            },
        ],
    }]
}

fn chain_with_looper(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![LooperConfig {
            output: Some(EndpointRef {
                binding_id: "io".into(),
                endpoint: "out1".into(),
            }),
            ..LooperConfig::new(UID)
        }],
    }
}

fn controller_for(chain: &Chain) -> ProjectRuntimeController {
    let registry = registry();
    let mut chains = HashMap::new();
    chains.insert(
        (chain.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(chain, 48_000.0, &[256], &registry).expect("runtime")),
    );
    let mut c =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    c.set_io_bindings(registry);
    c
}

/// One chain with one looper, a dispatcher that owns it, and this frontend's
/// looper doors attached to a real (device-less) controller. Nothing here is
/// GUI: an MCP client or a MIDI footswitch drives exactly this.
struct Rig {
    dispatcher: LocalDispatcher,
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    chain: Chain,
}

impl Rig {
    /// The project starts with NO looper: the fixture's looper is created by
    /// dispatching `AddChainLooper` (uid 1) and pointed at `out1` by
    /// dispatching `SetChainLooperOutput`, so every step of the setup is a
    /// command too.
    fn new(id: &str) -> Self {
        let shape = chain_with_looper(id);
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![Chain {
                loopers: vec![],
                ..shape.clone()
            }],
            midi: None,
        })));
        let runtime = LooperRuntimeControl::attach(&dispatcher, controller_for(&shape));
        let rig = Self {
            dispatcher,
            runtime,
            chain: shape,
        };
        rig.dispatch(LooperCommand::AddChainLooper {
            chain: rig.chain.id.clone(),
        });
        rig.dispatch(LooperCommand::SetChainLooperOutput {
            chain: rig.chain.id.clone(),
            looper: UID,
            output: Some(EndpointRef {
                binding_id: "io".into(),
                endpoint: "out1".into(),
            }),
        });
        rig
    }

    fn dispatch(&self, cmd: LooperCommand) {
        self.dispatcher
            .dispatch(Command::Looper(cmd))
            .unwrap_or_else(|e| panic!("looper command failed: {e}"));
    }

    fn transport(&self, action: LooperAction) {
        self.dispatch(LooperCommand::SetChainLooperTransport {
            chain: self.chain.id.clone(),
            looper: UID,
            action,
        });
    }

    fn with_controller<T>(&self, f: impl FnOnce(&ProjectRuntimeController) -> T) -> T {
        let borrow = self.runtime.borrow();
        f(borrow.as_ref().expect("controller"))
    }

    fn state(&self) -> Option<LooperState> {
        self.with_controller(|c| c.chain_looper_status(&self.chain.id, UID).map(|s| s.state))
    }

    fn stream_active(&self) -> bool {
        self.with_controller(|c| c.looper_stream_active(&self.chain.id, UID))
    }

    fn feed_input(&self, level: f32) {
        self.with_controller(|c| {
            let frames = 128usize;
            let input = vec![level; frames * 2];
            let mut out = vec![0.0f32; frames * 2];
            for rt in c.runtimes_for_chain(&self.chain.id) {
                engine::runtime::process_input_f32(&rt, 0, &input, 2);
                engine::runtime::process_output_f32(&rt, 0, &mut out, 2);
            }
        });
    }

    /// Record + close one loop through the COMMAND path + the input tap, so it
    /// plays.
    fn record_and_arm(&self) {
        self.transport(LooperAction::Record); // → Recording
        self.with_controller(|c| c.drain_looper_recording(&self.chain)); // subscribe tap
        self.feed_input(0.5); // fill ring
        self.with_controller(|c| c.drain_looper_recording(&self.chain)); // drain into loop
        self.transport(LooperAction::Record); // close → Playing
        assert!(
            self.stream_active(),
            "precondition: a closed loop arms its isolated stream"
        );
    }
}

/// The bug the MCP repro exposed: a looper driven by a NON-GUI transport
/// (MCP/MIDI) mutated nothing, so Record left the loop `Empty`. The command
/// itself now carries the mutation, so there is only one road left.
#[test]
fn a_looper_driven_off_the_gui_reaches_the_store() {
    let rig = Rig::new("wire-drain");
    rig.transport(LooperAction::Record);
    assert_eq!(
        rig.state(),
        Some(LooperState::Recording),
        "a looper transport dispatched off the GUI must reach the store, not \
         just emit an event"
    );
}

#[test]
fn adding_a_looper_creates_it_in_the_store() {
    let chain = Chain {
        loopers: vec![],
        ..chain_with_looper("wire-add")
    };
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    })));
    let runtime = LooperRuntimeControl::attach(&dispatcher, controller_for(&chain));
    dispatcher
        .dispatch(Command::Looper(LooperCommand::AddChainLooper {
            chain: chain.id.clone(),
        }))
        .expect("add");
    assert_eq!(
        runtime
            .borrow()
            .as_ref()
            .expect("controller")
            .chain_looper_status(&chain.id, UID)
            .map(|s| s.state),
        Some(LooperState::Empty),
        "the store holds the loop — a dispatch alone is dead (#614)"
    );
}

#[test]
fn record_records_then_closes_the_loop() {
    let rig = Rig::new("wire-record");
    rig.transport(LooperAction::Record);
    assert_eq!(rig.state(), Some(LooperState::Recording));
    rig.with_controller(|c| c.drain_looper_recording(&rig.chain));
    rig.feed_input(0.5);
    rig.with_controller(|c| c.drain_looper_recording(&rig.chain));
    rig.transport(LooperAction::Record);
    let s = rig
        .with_controller(|c| c.chain_looper_status(&rig.chain.id, UID))
        .expect("status");
    assert_eq!(s.state, LooperState::Playing);
    assert!(s.len_frames > 0, "the captured input defines the loop");
}

#[test]
fn stop_and_clear_reach_the_store() {
    let rig = Rig::new("wire-transport");
    rig.record_and_arm();

    rig.transport(LooperAction::Stop);
    assert_eq!(rig.state(), Some(LooperState::Stopped));

    rig.transport(LooperAction::Clear);
    let s = rig
        .with_controller(|c| c.chain_looper_status(&rig.chain.id, UID))
        .expect("status");
    assert_eq!(s.state, LooperState::Empty);
    assert_eq!(s.len_frames, 0);
}

#[test]
fn param_commands_reach_the_store_and_the_loop_keeps_playing() {
    let rig = Rig::new("wire-params");
    rig.record_and_arm();

    for param in [
        LooperParam::Mix(0.5),
        LooperParam::Decay(0.5),
        LooperParam::Speed(LooperSpeed::Double),
        LooperParam::Reverse(true),
    ] {
        rig.dispatch(LooperCommand::SetChainLooperParam {
            chain: rig.chain.id.clone(),
            looper: UID,
            param,
        });
    }
    let s = rig
        .with_controller(|c| c.chain_looper_status(&rig.chain.id, UID))
        .expect("status");
    assert_eq!(s.state, LooperState::Playing);
    assert!(rig.with_controller(|c| c.export_chain_looper(&rig.chain.id, UID).is_some()));
}

#[test]
fn stop_disarms_the_stream_even_when_the_chain_is_not_streaming() {
    // The redesign's core: stop is authoritative regardless of the chain
    // callback — and the reconcile that silences the loop is part of the door,
    // so no poll tick has to run first.
    let rig = Rig::new("wire-stop-no-stream");
    rig.record_and_arm();

    rig.transport(LooperAction::Stop);
    assert!(
        !rig.stream_active(),
        "stop silences the loop even when nothing is streaming"
    );
}

#[test]
fn play_after_stop_re_arms() {
    let rig = Rig::new("wire-replay");
    rig.record_and_arm();

    rig.transport(LooperAction::Stop);
    assert!(!rig.stream_active());

    rig.transport(LooperAction::PlayStop); // stopped → play
    assert!(
        rig.stream_active(),
        "play after stop must sound the loop again — and PlayStop is resolved \
         against the store, so the button and a footswitch agree"
    );
}

#[test]
fn removing_a_looper_frees_it_and_disarms_its_stream() {
    let rig = Rig::new("wire-remove");
    rig.record_and_arm();

    rig.dispatch(LooperCommand::RemoveChainLooper {
        chain: rig.chain.id.clone(),
        looper: UID,
    });
    assert!(rig.with_controller(|c| c.chain_looper_status(&rig.chain.id, UID).is_none()));
    assert!(
        !rig.stream_active(),
        "removing a loop stops its isolated stream"
    );
}
