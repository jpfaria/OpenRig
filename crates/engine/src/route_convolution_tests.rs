//! Unit tests for the issue #592 convolution-cushion policy helpers.

use super::block_is_convolution;

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, NamBlock};
use project::param::ParameterSet;

fn core(effect_type: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("m".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn detects_cab_block_as_convolution() {
    assert!(block_is_convolution(&core(
        block_core::EFFECT_TYPE_CAB,
        "ir_marshall_4x12_v30"
    )));
}

#[test]
fn detects_ir_prefixed_model_as_convolution() {
    assert!(block_is_convolution(&core("gain", "ir_weird")));
}

#[test]
fn plain_gain_chain_is_not_convolution() {
    assert!(!block_is_convolution(&core("gain", "fuzz_ge")));
}

#[test]
fn disabled_cab_does_not_count() {
    let mut cab = core(block_core::EFFECT_TYPE_CAB, "ir_x");
    cab.enabled = false;
    assert!(!block_is_convolution(&cab));
}

#[test]
fn nam_amp_is_not_convolution() {
    let nam = AudioBlock {
        id: BlockId("m".into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: "nam_marshall_plexi".into(),
            params: ParameterSet::default(),
        }),
    };
    assert!(!block_is_convolution(&nam));
}

// ── #965 defect 8: a mid tap only hears the blocks before it ──

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{InsertBlock, OutputBlock};
use project::chain::Chain;

use crate::runtime_graph::build_chain_runtime_state;
use crate::runtime_state::ChainRuntimeState;

fn endpoint(name: &str, dev: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![endpoint("in", "dev", ChannelMode::Mono, &[0])],
            outputs: vec![endpoint("out", "dev", ChannelMode::Stereo, &[0, 1])],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![endpoint("aux-out", "dev", ChannelMode::Stereo, &[2, 3])],
        },
        IoBinding {
            id: "loop".into(),
            name: "LOOP".into(),
            inputs: vec![endpoint("ret", "dev", ChannelMode::Stereo, &[4, 5])],
            outputs: vec![endpoint("snd", "dev", ChannelMode::Mono, &[6])],
        },
    ]
}

fn block(id: &str, kind: AudioBlockKind) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind,
    }
}

/// `[Insert, Drive, Output(mid tap), Cab]`: the tap sits BEFORE the cab in
/// the segment after the insert, so no convolver feeds it and it must not be
/// born with the IR cushion.
#[test]
fn a_mid_tap_before_the_cab_is_not_fed_by_the_convolver() {
    let owner = Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![
            block(
                "insert",
                AudioBlockKind::Insert(InsertBlock {
                    model: "standard".into(),
                    io: "loop".into(),
                }),
            ),
            core("gain", "volume"),
            block(
                "mid-out",
                AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: "aux".into(),
                    endpoint: "aux-out".into(),
                }),
            ),
            block(
                "cab",
                AudioBlockKind::Core(CoreBlock {
                    effect_type: block_core::EFFECT_TYPE_CAB.into(),
                    model: "ir_test_fake".into(),
                    params: ParameterSet::default(),
                }),
            ),
        ],
        di_output: None,
        loopers: vec![],
    };
    let rt: Arc<ChainRuntimeState> = Arc::new(
        build_chain_runtime_state(&owner, 48_000.0, &[128, 128, 64], &registry())
            .expect("the chain must build"),
    );
    let routes = rt.output_routes.load();
    let tail = routes[0].as_ref().expect("route 0 is the tail");
    let tap = routes[1].as_ref().expect("route 1 is the mid tap");
    assert_eq!(
        tail.buffer.len(),
        128,
        "the tail is behind the cab: born at its cushion"
    );
    assert_eq!(
        tap.buffer.len(),
        0,
        "the tap hears only the drive before the cab — no convolver feeds it"
    );
}
