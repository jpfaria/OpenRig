//! Issue #588 — a MONO source must not load a mono NAM model once per
//! stereo channel.
//!
//! OpenRig processes every stream as stereo internally (invariant #5): a
//! mono input is broadcast to `Stereo([s, s])`. A mono NAM amp placed on
//! such a chain currently gets instantiated once per channel, so the model
//! is loaded TWICE — but for a mono source the two channels are
//! bit-identical going into the amp, so the second instance produces
//! identical output at double the memory and CPU. With many mono NAM/IR
//! blocks in a rig this doubles their entire footprint.
//!
//! Contract: when the source feeding a mono model is mono (both stereo
//! channels carry the same signal), exactly one model instance is loaded.
//! The DualMono / true-stereo cases (channels differ) are out of scope for
//! this test and legitimately need per-channel processing.
//!
//! This test lives in its own integration binary so the process-global NAM
//! instance counter is not perturbed by other tests running in parallel.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Once;

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, NamBlock};
use project::chain::Chain;
use project::project::Project;

const SR: f32 = 48_000.0;

fn fixture_plugins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn init_test_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        plugin_loader::registry::init(&fixture_plugins_root());
    });
}

fn registry() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

/// MONO input (single channel) → NAM amp → stereo output.
fn mono_source_nam_chain(id: &str) -> Chain {
    let mut amp_params = ParameterSet::default();
    amp_params.insert("preset", ParameterValue::String("angus".into()));
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-588 mono source single model".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId("amp".into()),
            enabled: true,
            kind: AudioBlockKind::Nam(NamBlock {
                model: "nam_marshall_plexi".into(),
                params: amp_params,
            }),
        }],
    }
}

#[test]
fn mono_source_loads_a_mono_nam_model_only_once() {
    init_test_registry();

    assert!(
        plugin_loader::registry::find("nam_marshall_plexi").is_some(),
        "fixture plugin nam_marshall_plexi must be discoverable"
    );

    let chain = mono_source_nam_chain("issue-588-mono");
    let mut rates = HashMap::new();
    rates.insert(chain.id.clone(), SR);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    };

    let _graph =
        engine::runtime_graph::build_runtime_graph(&project, &rates, &HashMap::new(), &registry())
            .expect("mono-source NAM chain must build");

    assert_eq!(
        nam::live_models(),
        1,
        "issue #588: a mono source broadcast to stereo loaded the mono NAM \
         model {} times instead of once. The two stereo channels carry the \
         same signal, so a single mono processor must serve both — loading \
         it per channel doubles the model footprint for identical output.",
        nam::live_models()
    );
}
