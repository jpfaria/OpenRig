//! #771 owner bug "dei play e não saiu o som": on a TWO-binding chain the DI
//! loop substitutes only segment 0 (#699), and #716 routes each binding's
//! inputs only to that SAME binding's outputs — so a render aimed at the
//! second binding's output drained pure silence. The routed render must feed
//! the loop through the CHOSEN output's own binding, so EVERY pickable
//! output carries the loop.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::di_render::render_di_loop_routed;
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

#[test]
fn every_pickable_output_renders_the_loop_with_signal() {
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
        let rendered = render_di_loop_routed(&chain, &registry, pick.as_ref(), 48_000, &sine())
            .expect("render");
        let peak = rendered
            .frames
            .iter()
            .map(|f| f[0].abs().max(f[1].abs()))
            .fold(0.0f32, f32::max);
        assert!(
            peak > 0.1,
            "#771: output pick {pick:?} rendered SILENT (peak {peak}) — the \
             loop must be fed through the chosen output's own binding"
        );
    }
}
