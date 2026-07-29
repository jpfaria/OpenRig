//! #717/#771 — the DI stream exposes its OWN meter source, fully isolated
//! from the guitar runtime. Since #771 the DI is a pre-rendered playback
//! parked on the chosen output; its peaks are maintained by the output
//! callback's mix (unit-pinned in `di_playback`), and surfaced through
//! `di_playback_peaks` — never the guitar runtime's taps. Arming must leave
//! the guitar's taps silent (isolation #4); disarm clears the meter source.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        loopers: vec![],
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
fn armed_di_exposes_its_own_meter_source_isolated_from_guitar() {
    let (chain, registry) = chain_and_registry();
    let guitar = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("guitar runtime"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain.id.clone(), 0usize), guitar);
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry);

    let guitar_tap = controller
        .subscribe_stream_tap(&chain.id, 0, 8192)
        .expect("guitar stream tap");

    // A steady non-silent loop (~ -6 dBFS).
    let pcm = Arc::new(DiPcm::new(vec![0.5; 4800], 48_000, 1));
    controller.arm_di_stream(&chain, pcm).expect("arm DI");

    // The render parks off-thread; once parked the DI has its OWN meter
    // source (peaks the output callback's mix maintains — magnitude is
    // unit-pinned in `di_playback`).
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && controller.di_playback_peaks(&chain.id).is_none() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        controller.di_playback_peaks(&chain.id).is_some(),
        "#771: the armed DI must expose its OWN meter source (playback peaks)"
    );

    // Isolation #4: neither arming nor rendering may drive the GUITAR's taps.
    assert_eq!(
        pop_peak_dbfs(&guitar_tap),
        SILENT_DBFS,
        "the guitar tap must stay silent — the DI never rides the guitar stream"
    );

    controller.disarm_di_stream(&chain.id);
    assert!(
        !controller.di_stream_active(&chain.id),
        "disarm must tear the DI playback down"
    );
    assert!(
        controller.di_playback_peaks(&chain.id).is_none(),
        "disarm must clear the DI meter source"
    );
}
