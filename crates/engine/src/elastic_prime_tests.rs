//! Unit tests for the issue #592 convolution-cushion policy helpers.

use super::{
    chain_has_convolution, elastic_capacity_target, elastic_prime_frames,
    IR_COLD_START_CUSHION_FRAMES,
};

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, NamBlock};
use project::chain::Chain;
use project::param::ParameterSet;

fn io_chain(mid: AudioBlock) -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![mid],
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
    c.blocks[0].enabled = false;
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
fn capacity_floors_at_cold_start_cushion_for_convolution() {
    // buffer 64 → base 128 < cushion 512 ⇒ floored up to the cold-start
    // cushion (decoupled from the convolver partition size in #617).
    assert_eq!(
        elastic_capacity_target(128, true),
        IR_COLD_START_CUSHION_FRAMES
    );
    // A base already above the cushion is kept.
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

#[test]
fn prime_frames_full_truth_table() {
    // (target, is_initial_build, has_convolution) → expected. Only the
    // initial-build-AND-convolution corner primes; every other corner is 0.
    let cases = [
        (512usize, true, true, 512usize),
        (512, true, false, 0),
        (512, false, true, 0),
        (512, false, false, 0),
        (0, true, true, 0),   // initial convolution but nothing to prime
        (1, true, true, 1),   // minimal prime survives
        (256, false, false, 0),
        (usize::MAX, true, true, usize::MAX), // oversized prime is passed through
    ];
    for (target, init, conv, expected) in cases {
        assert_eq!(
            elastic_prime_frames(target, init, conv),
            expected,
            "elastic_prime_frames({target}, {init}, {conv})"
        );
    }
}

#[test]
fn capacity_target_boundaries_around_the_cushion() {
    let floor = IR_COLD_START_CUSHION_FRAMES; // 512
    // Convolution chains floor at the cushion; values around the boundary.
    assert_eq!(elastic_capacity_target(0, true), floor);
    assert_eq!(elastic_capacity_target(floor - 1, true), floor);
    assert_eq!(elastic_capacity_target(floor, true), floor);
    assert_eq!(elastic_capacity_target(floor + 1, true), floor + 1);
    assert_eq!(elastic_capacity_target(usize::MAX, true), usize::MAX);
    // Non-convolution keeps the lean base verbatim — no floor.
    assert_eq!(elastic_capacity_target(0, false), 0);
    assert_eq!(elastic_capacity_target(64, false), 64);
    assert_eq!(elastic_capacity_target(usize::MAX, false), usize::MAX);
}
