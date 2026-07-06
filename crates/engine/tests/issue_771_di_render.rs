//! Issue #771 — the DI loop is PRE-RENDERED through a copy of the chain's
//! block graph, off-thread, producing one steady-state loop period at the
//! chosen output's rate. The output callback later plays this buffer at its
//! own cursor (output-clocked), so the render must carry the loop's signal
//! and be exactly one period long.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::di_render::render_di_loop;
use engine::runtime::build_chain_runtime_state;
use engine::DiPcm;
use project::chain::Chain;

fn chain_and_registry() -> (Chain, Vec<IoBinding>) {
    let chain = Chain {
        id: ChainId("di771render".into()),
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

/// 1s of a 440 Hz mono sine at 44.1k — resampling to 48k is part of the deal
/// (#749 per-output-rate resample).
fn sine_pcm() -> DiPcm {
    let samples: Vec<f32> = (0..44_100)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.5)
        .collect();
    DiPcm::new(samples, 44_100, 1)
}

#[test]
fn rendered_loop_has_one_period_at_output_rate_and_carries_signal() {
    let (chain, registry) = chain_and_registry();
    let pcm = sine_pcm();
    let rendered = render_di_loop(&chain, &registry, 0, 48_000, &pcm).expect("render");

    let expected_len = pcm.to_loop_at(48_000).len();
    assert_eq!(
        rendered.frames.len(),
        expected_len,
        "#771: the rendered buffer must be exactly one loop period at the \
         output rate (48k), got {} vs {}",
        rendered.frames.len(),
        expected_len
    );
    assert_eq!(rendered.sample_rate, 48_000);

    let peak = rendered
        .frames
        .iter()
        .map(|f| f[0].abs().max(f[1].abs()))
        .fold(0.0f32, f32::max);
    assert!(
        peak > 0.1,
        "#771: the render must carry the loop's signal through the chain \
         copy, got peak {peak}"
    );
    assert!(
        rendered
            .frames
            .iter()
            .all(|f| f[0].is_finite() && f[1].is_finite()),
        "#771: rendered samples must be finite"
    );
}

#[test]
fn render_never_touches_the_guitar_runtime() {
    let (chain, registry) = chain_and_registry();
    let guitar = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("guitar runtime"),
    );

    let pcm = sine_pcm();
    render_di_loop(&chain, &registry, 0, 48_000, &pcm).expect("render");

    assert!(
        !guitar.has_di_loop(),
        "#771: pre-rendering builds its OWN runtime copy — the guitar runtime \
         must never see the loop (invariant #4)"
    );
}
