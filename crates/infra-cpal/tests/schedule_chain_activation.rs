//! Issue #672 — cold activation. `schedule_chain_activation` builds a chain's
//! runtime off the frontend thread (the NAM/IR load — the bulk of the activation
//! freeze) and returns true; the frontend poll then creates the cpal streams
//! (which are `!Send`, so they must stay on the frontend) and installs the chain.
//!
//! It only handles the single-input-group case (one `ChainRuntimeState`); a
//! multi-input chain needs N per-device runtimes and returns false so the caller
//! falls back to the synchronous build. That guard runs before any device probe,
//! so it is testable without hardware.

use std::collections::HashMap;

use domain::ids::{BlockId, ChainId, DeviceId};
use engine::runtime::RuntimeGraph;
use infra_cpal::ProjectRuntimeController;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;

fn input_on(device: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(format!("input:{device}")),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![InputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn output_stereo() -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId("out".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

#[test]
fn returns_false_for_a_multi_input_chain() {
    // Two distinct input devices => two per-device runtimes; the single-runtime
    // off-thread path does not apply, so it must defer to the synchronous build.
    let chain = Chain {
        id: ChainId("multi".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![input_on("devA"), input_on("devB"), output_stereo()],
    };
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    };
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: HashMap::new(),
    });

    let scheduled = controller
        .schedule_chain_activation(&project, &chain)
        .expect("multi-input query must not error");
    assert!(
        !scheduled,
        "a multi-input chain must fall back to the synchronous activation build"
    );
}
