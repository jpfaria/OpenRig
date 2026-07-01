//! Issue #740 — a multi-binding (e.g. four-binding) chain must be brought up
//! ASYNC on cold start, exactly like the single-input chain (#672/#693).
//!
//! The owner's live failure: one rig chain bound to FOUR isolated streams across
//! two interfaces floods underruns the instant playback begins
//! (`346752 new underrun(s)` at boot). Root cause: `schedule_chain_activation`
//! returns `false` for any multi-input chain, so cold start falls back to the
//! SYNCHRONOUS, serial build — each per-binding runtime/stream is opened one at a
//! time and the first streams already run their audio callback (counting
//! underruns) while the remaining heavy NAM/IR builds still block the thread.
//!
//! The owner's standing rule: stream/runtime bring-up must ALWAYS be
//! async/parallel — no stream's activation may block (or starve) a sibling's.
//!
//! This guard runs before any device probe (fake device ids, `for_testing`), so
//! it is deterministic and needs no hardware. The hardware battery counterpart
//! (real four-binding cold start, ~zero boot underruns) lives in the
//! `OPENRIG_HW_TESTS=1` battery.

use std::collections::HashMap;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::RuntimeGraph;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

fn mono_input(name: &str, device: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

fn binding(id: &str, input_device: &str, output_device: &str) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![mono_input(&format!("{id}-in"), input_device)],
        outputs: vec![IoEndpoint {
            name: format!("{id}-out"),
            device_id: DeviceId(output_device.into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

#[test]
fn four_binding_chain_is_scheduled_async_on_cold_start() {
    // The owner's shape: ONE chain, FOUR isolated input bindings across two
    // interfaces (Model A #716: the device endpoints live in the binding
    // registry, not block `entries`).
    let registry = vec![
        binding("io-1", "devA", "out"),
        binding("io-2", "devB", "out"),
        binding("io-3", "devC", "out"),
        binding("io-4", "devD", "out"),
    ];
    let chain = Chain {
        id: ChainId("rig".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![
            "io-1".into(),
            "io-2".into(),
            "io-3".into(),
            "io-4".into(),
        ],
        blocks: vec![],
        di_output: None,
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
    controller.set_io_bindings(registry);

    let scheduled = controller
        .schedule_chain_activation(&project, &chain)
        .expect("multi-binding activation query must not error");

    assert!(
        scheduled,
        "BUG #740: a four-binding chain must be brought up ASYNC on cold start \
         (its per-binding runtimes/streams built off the calling thread), not \
         fall back to the synchronous serial build that starves the first \
         streams while the rest come up — the owner's boot-time underrun flood."
    );
}
