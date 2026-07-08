//! Issue #771 — arming the DI pre-renders the loop and parks the playback on
//! the CHOSEN output's cell (resolved from `Chain.di_output`), at that
//! output's rate — never on the other outputs, never on the guitar runtime.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::DiPcm;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, DiOutputRef};

fn chain_and_registry(di_output: Option<DiOutputRef>) -> (Chain, Vec<IoBinding>) {
    let chain = Chain {
        id: ChainId("di771route".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output,
    };
    let out = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
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
        outputs: vec![out("out_main", vec![0, 1]), out("out_fx", vec![2, 3])],
    }];
    (chain, registry)
}

fn controller_for(
    chain: &Chain,
    registry: &[IoBinding],
    sample_rate: u32,
) -> (
    ProjectRuntimeController,
    Arc<engine::runtime::ChainRuntimeState>,
) {
    let guitar = Arc::new(
        build_chain_runtime_state(chain, sample_rate as f32, &[256], registry)
            .expect("guitar runtime"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain.id.clone(), 0usize), guitar.clone());
    let mut controller = ProjectRuntimeController::for_testing_with_sample_rate(
        RuntimeGraph { chains },
        sample_rate,
    );
    controller.set_io_bindings(registry.to_vec());
    (controller, guitar)
}

fn sine_pcm() -> Arc<DiPcm> {
    let samples: Vec<f32> = (0..22_050)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.5)
        .collect();
    Arc::new(DiPcm::new(samples, 44_100, 1))
}

/// The render runs on a short-lived off-thread; poll until it parks.
fn wait_for_playback(controller: &ProjectRuntimeController, chain_id: &ChainId) -> Option<usize> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(idx) = controller.di_playback_active_output(chain_id) {
            return Some(idx);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn playback_parks_on_the_chosen_output_only() {
    let chosen = DiOutputRef {
        binding_id: "io".into(),
        endpoint: "out_fx".into(),
    };
    let (chain, registry) = chain_and_registry(Some(chosen));
    let (controller, guitar) = controller_for(&chain, &registry, 48_000);

    controller
        .arm_di_stream(&chain, sine_pcm())
        .expect("arm DI stream");

    assert_eq!(
        wait_for_playback(&controller, &chain.id),
        Some(1),
        "#771: the pre-rendered playback must park on the CHOSEN output \
         (flat index 1, 'out_fx') — and on no other output"
    );
    assert!(
        !guitar.has_di_loop(),
        "#771: the guitar runtime must stay untouched (isolation #4)"
    );

    controller.disarm_di_stream(&chain.id);
    assert!(
        controller.di_playback_active_output(&chain.id).is_none(),
        "#771: disarm must clear the parked playback"
    );
    assert!(!controller.di_stream_active(&chain.id));
}

#[test]
fn playback_defaults_to_the_main_output_when_no_choice_persisted() {
    let (chain, registry) = chain_and_registry(None);
    let (controller, _guitar) = controller_for(&chain, &registry, 48_000);

    controller
        .arm_di_stream(&chain, sine_pcm())
        .expect("arm DI stream");

    assert_eq!(
        wait_for_playback(&controller, &chain.id),
        Some(0),
        "#771: di_output = None must keep today's default — the chain's \
         main (first) output"
    );
}

#[test]
fn playback_is_rendered_at_the_resolved_output_rate() {
    let (chain, registry) = chain_and_registry(None);
    let (controller, _guitar) = controller_for(&chain, &registry, 44_100);

    let pcm = sine_pcm();
    let expected_len = pcm.to_loop_at(44_100).len();
    controller
        .arm_di_stream(&chain, pcm)
        .expect("arm DI stream");

    wait_for_playback(&controller, &chain.id).expect("playback parked");
    assert_eq!(
        controller.di_stream_loop_len(&chain.id),
        Some(expected_len),
        "#771: the parked playback must be rendered at the resolved output \
         rate (#749 per-output-rate resample), not a hardcoded one"
    );
}
