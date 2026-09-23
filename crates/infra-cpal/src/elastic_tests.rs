//! #965 — elastic targets: one sizing for the cold build and the live rebuild.

use super::*;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};

use crate::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};

const QUANTUM: &str = "coreaudio:quantum";
const OTHER: &str = "coreaudio:other";

fn endpoint(name: &str, dev: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode,
        channels: channels.to_vec(),
    }
}

/// `guitarra-1` reads the guitar and plays Main on the Quantum; `violao`
/// reads the Quantum and plays on another interface; `syn2-main` is the
/// insert loop on the Quantum.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![endpoint("in", QUANTUM, ChannelMode::Mono, &[0])],
            outputs: vec![endpoint("main", QUANTUM, ChannelMode::Stereo, &[0, 1])],
        },
        IoBinding {
            id: "violao".into(),
            name: "Violao".into(),
            inputs: vec![endpoint("in", QUANTUM, ChannelMode::Mono, &[1])],
            outputs: vec![endpoint("out", OTHER, ChannelMode::Stereo, &[0, 1])],
        },
        IoBinding {
            id: "syn2-main".into(),
            name: "SYN2".into(),
            inputs: vec![endpoint("ret", QUANTUM, ChannelMode::Stereo, &[2, 3])],
            outputs: vec![endpoint("snd", QUANTUM, ChannelMode::Mono, &[7])],
        },
    ]
}

fn chain_on(binding: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![binding.into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn insert_chain() -> Chain {
    chain_on(
        "guitarra-1",
        vec![AudioBlock {
            id: BlockId("insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "syn2-main".into(),
            }),
        }],
    )
}

#[cfg_attr(all(target_os = "linux", feature = "jack"), allow(dead_code))]
fn signature(inputs: &[(&str, u32)], outputs: &[(&str, u32)]) -> ChainStreamSignature {
    ChainStreamSignature {
        inputs: inputs
            .iter()
            .map(|(dev, buf)| InputStreamSignature {
                device_id: (*dev).into(),
                channels: vec![0],
                stream_channels: 30,
                sample_rate: 44_100,
                buffer_size_frames: *buf,
            })
            .collect(),
        outputs: outputs
            .iter()
            .map(|(dev, buf)| OutputStreamSignature {
                device_id: (*dev).into(),
                channels: vec![0, 1],
                stream_channels: 30,
                sample_rate: 44_100,
                buffer_size_frames: *buf,
            })
            .collect(),
    }
}

/// Adversarial review of #965: the cold build sizes an insert SEND at the
/// lean insert multiplier (one buffer), but the live-rebuild path sized every
/// output — sends included — at the regular multiplier. The first knob turn
/// on a chain with an insert therefore rebuilt the send route (a gap in the
/// loop) and left it one buffer deeper than a fresh chain.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn a_live_rebuild_sizes_the_insert_send_like_the_cold_build() {
    let targets = elastic_targets_for_live_streams(
        &insert_chain(),
        &registry(),
        &signature(&[(QUANTUM, 64)], &[(QUANTUM, 64), (QUANTUM, 64)]),
    );
    assert_eq!(
        targets[1],
        engine::runtime::elastic_target_for_buffer(64, ELASTIC_MULTIPLIER_INSERT_SEND),
        "the insert send (last output) must keep its lean target across a live rebuild"
    );
}

/// Adversarial review of #965: the producer pushes a whole input callback
/// into a route at once. When the input device runs a bigger buffer than the
/// output's (guitar at 256 frames on one interface, output at 64), a cushion
/// sized from the output alone gives a ring (2x the cushion) that cannot even
/// hold one burst: drop-newest loses frames and the output underruns on half
/// its callbacks for the life of the chain.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn a_route_fed_by_a_bigger_input_buffer_rests_a_whole_burst_deep() {
    let targets = elastic_targets_for_live_streams(
        &chain_on("guitarra-1", vec![]),
        &registry(),
        &signature(&[(QUANTUM, 256)], &[(QUANTUM, 64)]),
    );
    assert!(
        targets[0] >= 256,
        "a 64-frame output fed by 256-frame input bursts rests at {} frames — \
         less than one burst",
        targets[0]
    );
}

/// #965, macOS: every CoreAudio unit of one device runs on ONE HAL IO thread,
/// input first and output 0-5 us later, every cycle (measured on the owner's
/// Quantum HD 8, both start orders). The chain DSP runs on the #670 worker,
/// so an output on the input's own device needs exactly one buffer for the
/// worker hand-off plus one of slack — the insert send has run live at that
/// cushion since #881 from the same worker pass. The second buffer the
/// regular multiplier added was pure latency on every same-device output.
#[cfg(target_os = "macos")]
#[test]
fn a_regular_output_on_the_input_device_rests_one_buffer_on_macos() {
    let targets = elastic_targets_for_live_streams(
        &chain_on("guitarra-1", vec![]),
        &registry(),
        &signature(&[(QUANTUM, 64)], &[(QUANTUM, 64)]),
    );
    assert_eq!(
        targets[0],
        engine::runtime::elastic_target_for_buffer(64, 1),
        "Main on the Quantum, fed from the Quantum"
    );
}

/// An output on ANOTHER interface runs on another clock and another HAL
/// thread: nothing orders its callbacks after the input's, so it keeps the
/// regular cushion.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn a_regular_output_on_another_device_keeps_the_regular_cushion() {
    let targets = elastic_targets_for_live_streams(
        &chain_on("violao", vec![]),
        &registry(),
        &signature(&[(QUANTUM, 64)], &[(OTHER, 64)]),
    );
    assert_eq!(
        targets[0],
        engine::runtime::elastic_target_for_buffer(64, ELASTIC_MULTIPLIER_REGULAR)
    );
}

fn registry_with_remote_loop() -> Vec<IoBinding> {
    let mut registry = registry();
    registry.push(IoBinding {
        id: "remote-loop".into(),
        name: "Loop on another interface".into(),
        inputs: vec![endpoint("ret", OTHER, ChannelMode::Stereo, &[0, 1])],
        outputs: vec![endpoint("snd", OTHER, ChannelMode::Mono, &[2])],
    });
    registry
}

/// Final review of `f92f8f10f`: the one-buffer cushion was granted when ANY
/// chain input shared the output's device. Guitar and Main on the Quantum,
/// insert loop on another interface: Main is written by the segment after
/// the insert, fed by the RETURN on the other interface's clock and HAL
/// thread — it must keep the regular cushion.
#[test]
fn a_tail_fed_by_a_return_on_another_device_keeps_the_regular_cushion() {
    let owner = chain_on(
        "guitarra-1",
        vec![AudioBlock {
            id: BlockId("insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "remote-loop".into(),
            }),
        }],
    );
    let targets = elastic_targets(
        &owner,
        &registry_with_remote_loop(),
        &[
            StreamClock {
                device_id: QUANTUM,
                buffer_frames: 64,
            },
            StreamClock {
                device_id: OTHER,
                buffer_frames: 64,
            },
        ],
        &[
            StreamClock {
                device_id: QUANTUM,
                buffer_frames: 64,
            },
            StreamClock {
                device_id: OTHER,
                buffer_frames: 64,
            },
        ],
    );
    assert_eq!(
        targets[0],
        engine::runtime::elastic_target_for_buffer(64, ELASTIC_MULTIPLIER_REGULAR),
        "Main is fed from the other interface's clock"
    );
    assert_eq!(
        targets[1],
        engine::runtime::elastic_target_for_buffer(64, ELASTIC_MULTIPLIER_INSERT_SEND),
        "the send keeps the lean insert cushion (it is not the same-device rule)"
    );
}

/// Final review of `f92f8f10f` (stream isolation law): the producer-burst
/// floor took the biggest buffer of EVERY chain input, so a second binding's
/// interface running 512 frames raised Main's cushion on the Quantum to 512.
/// A route is sized only by the stream that feeds it.
#[test]
fn a_route_is_not_sized_by_another_streams_buffer() {
    let mut registry = registry();
    registry.push(IoBinding {
        id: "other-guitar".into(),
        name: "Other guitar".into(),
        inputs: vec![endpoint("in", OTHER, ChannelMode::Mono, &[0])],
        outputs: vec![endpoint("out", OTHER, ChannelMode::Stereo, &[0, 1])],
    });
    let mut two = chain_on("guitarra-1", vec![]);
    two.io_binding_ids.push("other-guitar".into());
    let targets = elastic_targets(
        &two,
        &registry,
        &[
            StreamClock {
                device_id: QUANTUM,
                buffer_frames: 64,
            },
            StreamClock {
                device_id: OTHER,
                buffer_frames: 512,
            },
        ],
        &[
            StreamClock {
                device_id: QUANTUM,
                buffer_frames: 64,
            },
            StreamClock {
                device_id: OTHER,
                buffer_frames: 512,
            },
        ],
    );
    assert!(
        targets[0] <= engine::runtime::elastic_target_for_buffer(64, ELASTIC_MULTIPLIER_REGULAR),
        "Main on the Quantum rests at {} frames because another stream runs 512",
        targets[0]
    );
}
