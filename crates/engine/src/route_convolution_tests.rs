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
        disabled_endpoints: Default::default(),
        mix: Default::default(),
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

// ── #328: a cab inside a split path convolves into the split's routes ──

#[test]
fn a_cab_inside_a_split_path_counts_as_convolution() {
    use project::block::split_params::default_split_params;
    use project::block::{SplitBlock, SplitEnd};

    let split = AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params: default_split_params(),
            a: vec![core("gain", "fuzz_ge")],
            b: vec![core(block_core::EFFECT_TYPE_CAB, "ir_marshall_4x12_v30")],
        }),
    };
    assert!(
        block_is_convolution(&split),
        "path B's cab convolves into the split's output"
    );
}

/// #328 — a Y → A/B output hears only the paths that feed it. Path B's cab
/// must not give the output path A alone feeds the convolution cushion (a
/// deeper cushion is latency that output never needed); path B's own output
/// keeps it.
#[test]
fn a_y_output_counts_only_the_paths_that_feed_it() {
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::split_params::default_split_params;
    use project::block::{SplitBlock, SplitEnd};
    use project::chain::Chain;
    use project::endpoint_disables::{EndpointDisables, EndpointRef};

    use super::route_has_convolution;
    use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
    use crate::runtime_segments::split_chain_into_segments;

    let out = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    };
    let registry = vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![out("out-a", vec![0, 1]), out("out-b", vec![2, 3])],
    }];
    let off = |name: &str| EndpointRef {
        io: "main".into(),
        endpoint: name.into(),
    };
    let split = AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Y,
            params: default_split_params(),
            a: vec![core("gain", "volume")],
            b: vec![core(block_core::EFFECT_TYPE_CAB, "ir_marshall_4x12_v30")],
        }),
    };
    let chain = Chain {
        mix: Default::default(),
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![split],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables {
            inputs: vec![],
            outputs: vec![],
            path_a_outputs: vec![off("out-b")],
            path_b_outputs: vec![off("out-a")],
        },
    };
    let (ri, ro) = resolve_chain_io(&chain, &registry);
    let (ei, ci, sp, eg) = effective_inputs(&chain, &ri, &registry);
    let eo = effective_outputs(&chain, &ro, &registry);
    let segments = split_chain_into_segments(&chain, &ei, &ci, &sp, &eg, &eo, &registry);

    assert!(
        !route_has_convolution(&chain, &segments, 0),
        "#328: out-a hears path A only — path B's cab must not deepen its cushion"
    );
    assert!(
        route_has_convolution(&chain, &segments, 1),
        "#328: out-b hears path B's cab — it keeps the convolution cushion"
    );
}
