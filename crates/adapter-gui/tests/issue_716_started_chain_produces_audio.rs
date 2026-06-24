//! #716 RED — a chain that was related-to-a-binding and reopened must produce
//! audio when started, instead of underrunning and falsely flagging overload.
//!
//! User repro: starting the chain turns the meter RED with
//! "0 new xrun(s), 640 new underrun(s) — the rig is heavy for this buffer size".
//! 0 xruns means the rig is NOT computing heavily; 640 underruns means it is
//! producing NO samples. Root: the binding selection was lost on reopen
//! (io_binding_ids dropped by the rig), the chain is UNBOUND, no runtime is
//! built, so the output device starves → underruns → false "heavy rig" red.
//!
//! This test pins the audio-level expectation: a started, bound-then-reopened
//! chain must pass non-silent input through to its output.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_input_f32, process_output_f32};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::block::InputEntry;
use project::chain::ChainInputMode;
use project::rig::{RigInput, RigPreset, RigProject};

fn rig_with_chain() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert("p1".into(), RigPreset::from_legacy_blocks(Vec::new(), 100.0));
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId("dev_a".to_string()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: Vec::new(),
        },
    );
    RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

fn binding_main() -> IoBinding {
    IoBinding {
        id: "main".into(),
        name: "Main".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev_a".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev_a".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }
}

#[test]
fn started_reopened_chain_produces_audio_no_underrun() {
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::SetChainIoBindings {
            chain: ChainId("rig:in".to_string()),
            binding_ids: vec!["main".to_string()],
        })
        .expect("SetChainIoBindings must succeed");

    let reopened =
        engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reopened
        .chains
        .into_iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");

    let req = BuildRequest {
        chain,
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: vec![binding_main()],
    };
    let runtimes = build_chain_runtime(&req).expect("build must not error");

    // Drive non-silent input through the first input runtime and measure the
    // output energy. No runtime (or silent output) == the device underruns.
    let frames = 64usize;
    let input: Vec<f32> = vec![0.5; frames];
    let mut energy = 0.0_f32;
    if let Some((_, runtime)) = runtimes.first() {
        for _ in 0..16 {
            process_input_f32(runtime, 0, &input, 1);
        }
        for _ in 0..16 {
            let mut out = vec![0.0_f32; frames];
            process_output_f32(runtime, 0, &mut out, 1);
            energy += out.iter().map(|s| s.abs()).sum::<f32>();
        }
    }

    assert!(
        energy > 1e-2,
        "a started, bound-then-reopened chain must produce audio; got energy={energy:.6} \
         (runtimes built = {}). The binding selection was lost on reopen, the chain is unbound, \
         no runtime produces samples → the device underruns and the meter falsely flags \
         'the rig is heavy for this buffer size'",
        runtimes.len()
    );
}

#[test]
fn started_reopened_chain_has_a_producer_so_device_does_not_starve() {
    // Distinct angle: the underrun (red overload) is the OUTPUT device starving
    // because nothing feeds it. The reopened chain must yield at least one
    // runtime that owns an output route, i.e. a producer for the device.
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    dispatcher
        .dispatch(Command::SetChainIoBindings {
            chain: ChainId("rig:in".to_string()),
            binding_ids: vec!["main".to_string()],
        })
        .expect("SetChainIoBindings must succeed");

    let reopened =
        engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reopened
        .chains
        .into_iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");

    let req = BuildRequest {
        chain,
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: vec![binding_main()],
    };
    let runtimes = build_chain_runtime(&req).expect("build must not error");

    assert!(
        !runtimes.is_empty(),
        "the started chain must have at least one runtime feeding the output device; got 0 — \
         with no producer the device underruns and the meter goes red ('640 underruns')"
    );
}
