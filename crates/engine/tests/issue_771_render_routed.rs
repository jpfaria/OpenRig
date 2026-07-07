//! #771 owner bug "dei play e não saiu o som": on a TWO-binding chain the DI
//! loop substitutes only segment 0 (#699), and #716 routes each binding's
//! inputs only to that SAME binding's outputs — so the second binding's
//! output route drained pure silence. The routed DI runtime must feed the
//! loop through the CHOSEN output's own binding, so EVERY pickable output
//! carries the loop.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::di_render::build_routed_di_runtime;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::DiPcm;
use project::chain::{Chain, DiOutputRef};

fn ep(name: &str, dev: &str, mode: ChannelMode, ch: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode,
        channels: ch,
    }
}

fn rig() -> (Chain, Vec<IoBinding>) {
    let chain = Chain {
        id: ChainId("user_rig".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlett".into(), "teyun".into()],
        blocks: vec![],
        di_output: None,
    };
    let registry = vec![
        IoBinding {
            id: "scarlett".into(),
            name: "SCARLET".into(),
            inputs: vec![
                ep("In 1", "scar", ChannelMode::Mono, vec![0]),
                ep("In 2", "scar", ChannelMode::Mono, vec![1]),
            ],
            outputs: vec![ep("Out 1", "scar", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "teyun".into(),
            name: "TEYUN".into(),
            inputs: vec![
                ep("In 1", "tey", ChannelMode::Mono, vec![0]),
                ep("In 2", "tey", ChannelMode::Mono, vec![1]),
                ep("In 3", "tey", ChannelMode::Mono, vec![2]),
            ],
            outputs: vec![ep("Out 1", "tey", ChannelMode::Stereo, vec![0, 1])],
        },
    ];
    (chain, registry)
}

fn sine() -> DiPcm {
    let samples: Vec<f32> = (0..44_100)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.5)
        .collect();
    DiPcm::new(samples, 44_100, 1)
}

/// Step the routed runtime a few blocks and return the drained peak.
fn drained_peak(routed: &engine::di_render::RoutedDiRuntime) -> f32 {
    const BLOCK: usize = 256;
    let silence = vec![0.0f32; BLOCK];
    let mut drain = vec![0.0f32; BLOCK * routed.drain_width];
    let mut peak = 0.0f32;
    for _ in 0..40 {
        process_input_f32(&routed.runtime, 0, &silence, 1);
        process_output_f32(
            &routed.runtime,
            routed.output_index,
            &mut drain,
            routed.drain_width,
        );
        for frame in drain.chunks(routed.drain_width) {
            peak = peak
                .max(frame[routed.drain_left].abs())
                .max(frame[routed.drain_right].abs());
        }
    }
    peak
}

#[test]
fn every_pickable_output_streams_the_loop_with_signal() {
    let (chain, registry) = rig();
    let picks = [
        None,
        Some(DiOutputRef {
            binding_id: "scarlett".into(),
            endpoint: "Out 1".into(),
        }),
        Some(DiOutputRef {
            binding_id: "teyun".into(),
            endpoint: "Out 1".into(),
        }),
    ];
    for pick in picks {
        let routed = build_routed_di_runtime(&chain, &registry, pick.as_ref(), 48_000, &sine())
            .expect("build routed runtime");
        let peak = drained_peak(&routed);
        assert!(
            peak > 0.1,
            "#771: output pick {pick:?} streams SILENCE (peak {peak}) — the \
             loop must be fed through the chosen output's own binding"
        );
    }
}

/// A chain with no bound outputs keeps its implicit default route.
#[test]
fn unbound_chain_streams_on_the_default_route() {
    let (mut chain, registry) = rig();
    chain.io_binding_ids.clear();
    let routed = build_routed_di_runtime(&chain, &registry, None, 48_000, &sine())
        .expect("build routed runtime");
    assert!(drained_peak(&routed) > 0.1);
}
