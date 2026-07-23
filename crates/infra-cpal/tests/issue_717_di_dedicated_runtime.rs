//! Issue #717 — the armed DI loop must play on a DEDICATED, isolated runtime,
//! NOT be injected into the guitar's runtime.
//!
//! Owner report: "ele ta usando o mesmo stream da minha guitarra… o gráfico
//! entrega." Today arming routes the loop into the guitar's per-input runtime
//! (`set_chain_di_loop` → `has_di_loop()` true on the guitar runtime). This
//! test pins the target behaviour, mechanism-agnostic: after arming, the guitar
//! runtime carries NO loop, and a separate DI runtime is playing.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::DiPcm;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

fn chain_and_registry() -> (Chain, Vec<IoBinding>) {
    let chain = Chain {
        id: ChainId("di717".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    (chain, registry)
}

#[test]
fn di_arms_on_a_separate_runtime_not_the_guitar_runtime() {
    let (chain, registry) = chain_and_registry();
    let guitar = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("guitar runtime"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain.id.clone(), 0usize), guitar.clone());
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry);

    let pcm = Arc::new(DiPcm::new(vec![0.2; 4800], 48_000, 1));
    // The DI stream is built from a copy of the chain's block graph, resolved
    // against the controller's own binding registry.
    controller
        .arm_di_stream(&chain, pcm)
        .expect("arm DI stream");

    assert!(
        !guitar.has_di_loop(),
        "#717: arming the DI must NOT put the loop on the guitar runtime — that \
         is the owner's complaint (the DI rides the guitar's stream/meters)"
    );
    assert!(
        controller.di_stream_active(&chain.id),
        "#717: a dedicated, isolated DI runtime must be playing for the chain"
    );
    // #771: the loop is pre-rendered on a short-lived off-thread and parks on
    // the chosen output's cell when done — poll until it does.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline && controller.di_stream_loop_len(&chain.id).is_none()
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        controller.di_stream_loop_len(&chain.id).is_some(),
        "#717: the dedicated DI stream must actually carry the loop (a real \
         pre-rendered playback, not just a flag)"
    );
}
