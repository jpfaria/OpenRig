//! #771 review findings — DI arm/disarm lifecycle invariants:
//! 1. mono-output DI must not be summed twice (+6 dB vs the chain path);
//! 2. a disarm ALWAYS wins over an in-flight render (no zombie playback);
//! 3. a failed render must not report the DI as playing forever;
//! 4. a rebuild re-arm re-parks the playback from the stored source.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::DiPcm;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

fn chain_and_registry(mono_out: bool) -> (Chain, Vec<IoBinding>) {
    let chain = Chain {
        id: ChainId("di771life".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    };
    let out = if mono_out {
        IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }
    } else {
        IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }
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
        outputs: vec![out],
    }];
    (chain, registry)
}

fn controller_for(chain: &Chain, registry: &[IoBinding]) -> ProjectRuntimeController {
    let guitar = Arc::new(
        build_chain_runtime_state(chain, 48_000.0, &[256], registry).expect("guitar runtime"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain.id.clone(), 0usize), guitar);
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry.to_vec());
    controller
}

fn long_pcm() -> Arc<DiPcm> {
    // 10 s — long enough that a disarm can land while the render runs.
    let samples: Vec<f32> = (0..441_000)
        .map(|i| (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 44_100.0).sin() * 0.5)
        .collect();
    Arc::new(DiPcm::new(samples, 44_100, 1))
}

fn wait_parked(controller: &ProjectRuntimeController, chain_id: &ChainId) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if controller.di_playback_active_output(chain_id).is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// 2 — a disarm ALWAYS wins over an in-flight render: after stop, no zombie
/// playback may ever park, no matter how the timing lands.
#[test]
fn disarm_always_wins_over_an_in_flight_render() {
    let (chain, registry) = chain_and_registry(false);
    let controller = controller_for(&chain, &registry);
    let pcm = long_pcm();

    for i in 0..40 {
        controller
            .arm_di_stream(&chain, pcm.clone())
            .expect("arm DI");
        // Let the render progress a random-ish amount before stopping.
        std::thread::sleep(Duration::from_millis(5 + (i % 13) * 7));
        controller.disarm_di_stream(&chain.id);
        assert!(!controller.di_stream_active(&chain.id));
    }
    // Give every detached render thread time to finish; NONE may have parked.
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        controller.di_playback_active_output(&chain.id).is_none(),
        "#771 zombie: a render that lost the disarm race parked its playback \
         — the DI keeps playing with the UI showing stopped"
    );
}

/// 3 — a render that FAILS must not leave the DI reported as playing forever.
#[test]
fn failed_render_reports_not_playing() {
    let (chain, registry) = chain_and_registry(false);
    let controller = controller_for(&chain, &registry);

    // An EMPTY source cannot render — the failure must surface.
    let empty = Arc::new(DiPcm::new(Vec::new(), 44_100, 1));
    controller
        .arm_di_stream(&chain, empty)
        .expect("arm returns Ok; the failure is surfaced via di_stream_active");

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && controller.di_stream_active(&chain.id) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !controller.di_stream_active(&chain.id),
        "#771: a failed render must flip the DI back to not-playing — \
         otherwise the UI shows an eternal silent 'playing' state"
    );
}

/// 4 — the controller can re-arm from the stored source after a rebuild.
#[test]
fn rearm_after_rebuild_reparks_from_stored_source() {
    let (chain, registry) = chain_and_registry(false);
    let controller = controller_for(&chain, &registry);

    controller
        .arm_di_stream(&chain, long_pcm())
        .expect("arm DI");
    assert!(wait_parked(&controller, &chain.id), "initial park");

    // A rebuild invalidates the parked playback; the controller must be able
    // to re-arm from what it stored — no dispatcher round-trip.
    controller.rearm_di_stream_after_rebuild(&chain);
    assert!(
        wait_parked(&controller, &chain.id),
        "#771: after a rebuild the DI must re-park from the stored source"
    );
    assert!(controller.di_stream_active(&chain.id));
}
