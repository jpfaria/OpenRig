//! Red-first: the console's tick must surface completions of off-thread
//! command work, exactly like the GUI's does.
//!
//! Symptom (found validating #791 over MCP): with the console adapter mounted,
//! `set_chain_di_loop_source` and `diagnose_chain_tone` accept the command and
//! then nothing ever happens — the DI never installs, the verdict never lands,
//! and a client keeps reading `{"tone": null}` forever. The heavy work runs on
//! its own task (#693) and its result reaches the world only through
//! `poll_async_results`, which the GUI's 16 ms timer calls and the console's
//! loop did not. Every async command is therefore dead on this transport.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;

use application::command::{Command, ToneDoctorCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::tick;

const SR: f32 = 48_000.0;
const CHAIN: &str = "chain:0";

fn init() {
    static I: Once = Once::new();
    I.call_once(engine::native_registry::register_all_natives);
}

fn params(model: &str, values: &[(&str, f32)]) -> ParameterSet {
    let schema = schema_for_block_model("gain", model).expect("gain schema");
    let mut ps = ParameterSet::default();
    for (k, v) in values {
        ps.insert(*k, ParameterValue::Float(*v));
    }
    ps.normalized_against(&schema).expect("params normalize")
}

fn block(id: &str, model: &str, ps: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ps,
        }),
    }
}

fn dispatcher_with_fizzy_chain() -> LocalDispatcher {
    init();
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId(CHAIN.into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![
                block("vol", "volume", params("volume", &[("volume", 80.0)])),
                block(
                    "fz",
                    "fuzz_si",
                    params(
                        "fuzz_si",
                        &[("fuzz", 95.0), ("tone", 90.0), ("level", 50.0)],
                    ),
                ),
            ],
            di_output: None,
        }],
        midi: None,
    }));
    LocalDispatcher::new(project)
}

fn sine() -> Vec<[f32; 2]> {
    (0..SR as usize)
        .map(|i| {
            let s = 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn a_tick_surfaces_completions_of_off_thread_command_work() {
    let dispatcher = dispatcher_with_fizzy_chain();
    dispatcher.attach_tone_doctor_input(Box::new(|_chain, _seconds| {
        Some(Box::new(|| Some((sine(), SR))))
    }));

    // A command whose work runs off-thread: the dispatch itself returns nothing.
    let immediate = dispatcher
        .dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
            chain: ChainId(CHAIN.into()),
            genre: None,
            seconds: Some(1),
        }))
        .expect("diagnose dispatches");
    assert!(
        immediate.is_empty(),
        "the verdict cannot be ready synchronously: {immediate:?}"
    );

    // Ticking is the console's only heartbeat, so the completion must come out
    // of it — otherwise the command is dead for every client on this transport.
    let mut seen = Vec::new();
    for _ in 0..600 {
        seen.extend(tick(&dispatcher, Vec::new()));
        if seen
            .iter()
            .any(|e| matches!(e, Event::ChainToneDiagnosed { .. }))
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!(
        "the tick never surfaced the finished diagnosis — off-thread command \
         work (DI decode, offline render, tone diagnosis) is invisible on the \
         console transport: {seen:?}"
    );
}

#[test]
fn a_tick_keeps_the_events_drained_from_the_bridge() {
    let dispatcher = dispatcher_with_fizzy_chain();
    let from_bridge = vec![Event::ProjectMutated];

    let out = tick(&dispatcher, from_bridge);

    assert_eq!(
        out,
        vec![Event::ProjectMutated],
        "the tick must not drop what the bridge already drained"
    );
}
