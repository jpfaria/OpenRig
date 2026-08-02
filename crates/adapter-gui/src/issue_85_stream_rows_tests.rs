//! #85 — one meter row per STREAM, so a mid port has its own bar.
//!
//! The chain row draws an INPUT/OUTPUT pair per stream ("para eu saber se tá
//! clipando ou não"). The row count came from the number of resolved INPUT
//! endpoints, so a chain with one guitar and a mid `Output` drew a single pair
//! and the port's stream — the one actually feeding the other interface — had
//! no meter at all.
//!
//! A stream is one (input × output) pipeline (see
//! `engine::runtime_segments`), which is also what the engine indexes its
//! per-stream taps by.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::meter_wiring::project_stream_count;

fn endpoint(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("scarlett".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![endpoint("main-out", "scarlett", vec![0, 1])],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![IoEndpoint {
                name: "aux-in".into(),
                device_id: DeviceId("teyun".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![endpoint("aux-out", "teyun", vec![2, 3])],
        },
    ]
}

fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-out".into(),
        }),
    }
}

fn mid_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-in".into(),
        }),
    }
}

fn chain_with(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("issue-85-rows".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn a_plain_chain_still_draws_one_row() {
    let chain = chain_with(vec![effect("A")]);
    assert_eq!(project_stream_count(&chain, &registry()), 1);
}

#[test]
fn a_mid_output_gets_its_own_row() {
    let chain = chain_with(vec![effect("A"), mid_output(), effect("B")]);
    assert_eq!(
        project_stream_count(&chain, &registry()),
        2,
        "#85: one guitar and a mid Output are TWO streams — the port needs its \
         own meter or the user cannot see it clipping"
    );
}

#[test]
fn a_mid_input_and_a_mid_output_are_four_rows() {
    let chain = chain_with(vec![
        effect("A"),
        mid_input(),
        effect("B"),
        mid_output(),
        effect("C"),
    ]);
    assert_eq!(
        project_stream_count(&chain, &registry()),
        4,
        "#85: 2 inputs × 2 outputs = 4 streams, so 4 meter rows"
    );
}
