//! #717 — the dedicated DI runtime must produce its OWN meters when driven,
//! fully isolated from the guitar runtime. Meters are SPSC taps filled only
//! inside the runtime's processing call, so the DI runtime yields levels only
//! once a driver clocks it (`di_drive_once` — the per-buffer step the DI worker
//! runs). Driving the DI must NOT feed the guitar runtime's taps (isolation #4).

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
fn driven_di_runtime_produces_its_own_meters_isolated_from_guitar() {
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

    // Subscribe the DI runtime's own output stream tap + the guitar's.
    let di_tap = controller
        .di_subscribe_stream_tap(&chain.id, 0, 8192)
        .expect("DI runtime must expose a stream tap");
    let guitar_tap = controller
        .subscribe_stream_tap(&chain.id, 0, 8192)
        .expect("guitar stream tap");

    // Clock the DI runtime as the worker will, one buffer at a time.
    for _ in 0..40 {
        controller.di_drive_once(&chain.id, 256);
    }

    assert!(
        pop_peak_dbfs(&di_tap) > SILENT_DBFS,
        "the driven DI runtime must produce its own meter signal from the loop"
    );
    assert_eq!(
        pop_peak_dbfs(&guitar_tap),
        SILENT_DBFS,
        "the guitar tap must stay silent — driving the DI must never feed the guitar runtime"
    );
}
