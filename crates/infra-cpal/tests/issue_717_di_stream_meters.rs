//! #717 — the dedicated DI runtime must produce its OWN meters, live, fully
//! isolated from the guitar runtime. Levels are SPSC taps filled only inside the
//! runtime's processing call, so arming spawns a worker that clocks the DI
//! runtime buffer by buffer; the armed loop substitutes the silent device input,
//! so the taps go live on their own. Driving the DI must never feed the guitar
//! runtime's taps (isolation #4), and disarm tears the worker + runtime down.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::output_meter::{pop_peak_dbfs, SILENT_DBFS};
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
fn armed_di_runtime_meters_go_live_isolated_from_guitar() {
    let (chain, registry) = chain_and_registry();
    let guitar = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("guitar runtime"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain.id.clone(), 0usize), guitar);
    let controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);

    // A steady non-silent loop (~ -6 dBFS).
    let pcm = Arc::new(DiPcm::new(vec![0.5; 4800], 48_000, 1));
    controller.arm_di_stream(&chain, pcm, &registry).expect("arm DI");

    // Subscribe the DI runtime's own output tap + the guitar's. The worker
    // spawned by arm re-loads the runtime each buffer, so a tap subscribed after
    // arm still fills.
    let di_tap = controller
        .di_subscribe_stream_tap(&chain.id, 0, 8192)
        .expect("DI stream tap");
    let guitar_tap = controller
        .subscribe_stream_tap(&chain.id, 0, 8192)
        .expect("guitar stream tap");

    // The worker drives the DI runtime on its own — meters go live with no
    // manual clock, within a short window.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut di_peak = SILENT_DBFS;
    while std::time::Instant::now() < deadline {
        di_peak = pop_peak_dbfs(&di_tap);
        if di_peak > SILENT_DBFS {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        di_peak > SILENT_DBFS,
        "arming must spawn a worker that drives the DI runtime → its own live meters"
    );
    assert_eq!(
        pop_peak_dbfs(&guitar_tap),
        SILENT_DBFS,
        "the guitar tap must stay silent — the DI runtime is fully isolated"
    );

    controller.disarm_di_stream(&chain.id);
    assert!(
        !controller.di_stream_active(&chain.id),
        "disarm must tear the DI runtime + its worker down"
    );
}
