//! Issue #588 — boundary guard for the mono-collapse optimization.
//!
//! The companion test `issue_588_mono_nam_single_instance` asserts that a
//! MONO source loads a `DualMono` NAM model once. This test guards the other
//! side of that boundary: a DualMono source carries two INDEPENDENT channels,
//! so the model must still be instantiated per channel (two live instances).
//! Collapsing here would silently sum/duplicate the channels and destroy the
//! independent stereo signal — a correctness regression, not an optimization.
//!
//! Own integration binary so the process-global NAM counter is isolated.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Once;

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, NamBlock, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
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

/// DualMono input (two independent channels) → NAM amp → stereo output.
fn dualmono_source_nam_chain(id: &str) -> Chain {
    let mut amp_params = ParameterSet::default();
    amp_params.insert("preset", ParameterValue::String("angus".into()));
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-588 dual-mono source keeps two instances".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::DualMono,
                        channels: vec![0, 1],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("amp".into()),
                enabled: true,
                kind: AudioBlockKind::Nam(NamBlock {
                    model: "nam_marshall_plexi".into(),
                    params: amp_params,
                }),
            },
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

#[test]
fn dualmono_source_keeps_independent_per_channel_nam_instances() {
    init_test_registry();

    assert!(
        plugin_loader::registry::find("nam_marshall_plexi").is_some(),
        "fixture plugin nam_marshall_plexi must be discoverable"
    );

    let chain = dualmono_source_nam_chain("issue-588-dualmono");
    let mut rates = HashMap::new();
    rates.insert(chain.id.clone(), SR);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    };

    let _graph = engine::runtime_graph::build_runtime_graph(&project, &rates, &HashMap::new())
        .expect("dual-mono-source NAM chain must build");

    assert_eq!(
        nam::live_models(),
        2,
        "issue #588: a DualMono source has independent L/R channels, so the \
         NAM model must run per channel (2 instances). The mono-collapse \
         optimization must NOT apply here — found {} instance(s).",
        nam::live_models()
    );
}
