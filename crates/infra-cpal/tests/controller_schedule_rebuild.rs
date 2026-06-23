//! Issue #672 — `ProjectRuntimeController::schedule_chain_rebuild` enqueues an
//! off-thread runtime build; `poll_pending_rebuilds` (called on the frontend
//! tick) applies the finished build by swapping the live slot AND the
//! runtime_graph in lock-step, so both stay consistent. The heavy build never
//! blocks the caller.
//!
//! Clean break (#716): routing is binding-only, so the rebuilt chain must be
//! BOUND — the controller is seeded with the matching registry.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;

fn one_binding() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "Interface".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn bound_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: "io".into(),
                    endpoint: "in".into(),
                    entries: Vec::new(),
                }),
            },
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: "io".into(),
                    endpoint: "out".into(),
                    entries: Vec::new(),
                }),
            },
        ],
    }
}

#[test]
fn schedule_then_poll_publishes_a_new_runtime_offthread() {
    let chain_id = ChainId("chain:672:rebuild".into());
    let chain = bound_chain(&chain_id.0);

    let initial = Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[1024]).unwrap());
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0_usize), Arc::clone(&initial));
    let graph = RuntimeGraph { chains };

    let mut controller = ProjectRuntimeController::for_testing(graph);
    controller.set_io_bindings(one_binding());

    let before = controller
        .chain_runtime(&chain_id)
        .expect("runtime present");
    assert!(Arc::ptr_eq(&before, &initial));

    // Enqueue the rebuild — must return immediately (no build on this thread).
    controller.schedule_chain_rebuild(&chain, 48_000.0, vec![1024]);

    // The frontend tick drains finished builds; spin the poll until applied.
    let mut applied = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while applied == 0 {
        applied += controller.poll_pending_rebuilds();
        assert!(
            std::time::Instant::now() < deadline,
            "rebuild never completed"
        );
        std::thread::yield_now();
    }
    assert_eq!(applied, 1);

    let after = controller
        .chain_runtime(&chain_id)
        .expect("runtime present");
    assert!(
        !Arc::ptr_eq(&before, &after),
        "poll must publish the freshly built runtime (new Arc)"
    );
}
