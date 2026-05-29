//! Unit tests for the issue #592 convolution-cushion policy helpers.

use super::{chain_has_convolution, elastic_capacity_target, elastic_prime_frames};

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, NamBlock, OutputBlock,
    OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;

fn io_chain(mid: AudioBlock) -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("d".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            mid,
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("d".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

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
    assert!(chain_has_convolution(&io_chain(core(
        block_core::EFFECT_TYPE_CAB,
        "ir_marshall_4x12_v30"
    ))));
}

#[test]
fn detects_ir_prefixed_model_as_convolution() {
    assert!(chain_has_convolution(&io_chain(core("gain", "ir_weird"))));
}

#[test]
fn plain_gain_chain_is_not_convolution() {
    assert!(!chain_has_convolution(&io_chain(core("gain", "fuzz_ge"))));
}

#[test]
fn disabled_cab_does_not_count() {
    let mut c = io_chain(core(block_core::EFFECT_TYPE_CAB, "ir_x"));
    c.blocks[1].enabled = false;
    assert!(!chain_has_convolution(&c));
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
    assert!(!chain_has_convolution(&io_chain(nam)));
}

#[test]
fn capacity_floors_at_partition_for_convolution() {
    // buffer 64 → base 128 < partition 512 ⇒ floored up.
    assert_eq!(elastic_capacity_target(128, true), ir::PARTITION_SIZE);
    // A base already above the partition is kept.
    assert_eq!(elastic_capacity_target(2048, true), 2048);
    // Non-convolution keeps the lean base unchanged.
    assert_eq!(elastic_capacity_target(128, false), 128);
}

#[test]
fn primes_only_on_initial_convolution_build() {
    assert_eq!(elastic_prime_frames(512, true, true), 512);
    assert_eq!(
        elastic_prime_frames(512, false, true),
        0,
        "rebuild: no prime"
    );
    assert_eq!(
        elastic_prime_frames(512, true, false),
        0,
        "non-IR: no prime"
    );
}
