//! #328 — Y → A/B routing keeps the one-segment-per-(input × output) model
//! (#85). The segment of output O runs the shared blocks and then the paths
//! whose output node has O checked: `ChainSegment.paths`. Both paths on one
//! output are ONE segment (the builder sums them inside it, aligned) — two
//! segments would be summed on the route, outside the mixer.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, OutputBlock, PathRef, SplitBlock, SplitEnd,
};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};
use project::param::ParameterSet;

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};
use crate::segment_types::SegmentPaths;

fn mono(name: &str, device: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

fn stereo(name: &str, device: &str, channels: [usize; 2]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels: channels.to_vec(),
    }
}

/// `main` is the chain's E/S; `aux` is another interface a mid Output uses.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![mono("in", "dev", 0)],
            outputs: vec![
                stereo("out-a", "dev", [0, 1]),
                stereo("out-b", "dev", [2, 3]),
                stereo("out-ab", "dev", [4, 5]),
            ],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![stereo("aux-out", "aux", [6, 7])],
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

fn y_split() -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::with_paths(
            SplitEnd::Y,
            vec![vec![effect("amp-a")], vec![effect("amp-b")]],
        )),
    }
}

/// Two amps summed by a Split → Mix.
fn mix_split() -> AudioBlock {
    AudioBlock {
        id: BlockId("mix".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::with_paths(
            SplitEnd::Mix,
            vec![vec![effect("amp-1")], vec![effect("amp-2")]],
        )),
    }
}

/// The owner's tail: path A a cab, path B nothing.
fn cab_or_nothing_y() -> AudioBlock {
    AudioBlock {
        id: BlockId("y".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::with_paths(
            SplitEnd::Y,
            vec![vec![effect("cab")], vec![]],
        )),
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

fn r(io: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: io.into(),
        endpoint: endpoint.into(),
    }
}

fn leaf(split: &str, path: usize) -> PathRef {
    PathRef {
        split: BlockId(split.into()),
        path,
    }
}

/// The segment of an output that runs these leaves of `split`.
fn only(split: &str, paths: &[usize]) -> SegmentPaths {
    SegmentPaths::Only(paths.iter().map(|&p| leaf(split, p)).collect())
}

/// Each leaf of `split` with the endpoints its output node unchecks.
fn leaf_disables(split: &str, unchecked: &[&[EndpointRef]]) -> EndpointDisables {
    let mut disables = EndpointDisables::default();
    for (path, refs) in unchecked.iter().enumerate() {
        for r in refs.iter() {
            disables.set_enabled(
                &EndpointNode::PathOutput(leaf(split, path)),
                r.clone(),
                false,
            );
        }
    }
    disables
}

/// Path A → out-a + out-ab; path B → out-b + out-ab.
fn y_disables() -> EndpointDisables {
    leaf_disables("split", &[&[r("main", "out-b")], &[r("main", "out-a")]])
}

fn chain(
    bindings: &[&str],
    blocks: Vec<AudioBlock>,
    disabled_endpoints: EndpointDisables,
) -> Chain {
    Chain {
        mix: Default::default(),
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn segments(chain: &Chain, registry: &[IoBinding]) -> (Vec<ChainSegment>, Vec<Vec<usize>>) {
    let (ri, ro) = resolve_chain_io(chain, registry);
    let (ei, ci, sp, eg) = effective_inputs(chain, &ri, registry);
    let eo = effective_outputs(chain, &ro, registry);
    let segs = split_chain_into_segments(chain, &ei, &ci, &sp, &eg, &eo, registry);
    let outs = eo.iter().map(|o| o.channels.clone()).collect();
    (segs, outs)
}

/// `(output channels, paths)` per segment — the routing, readably.
fn routing(chain: &Chain, registry: &[IoBinding]) -> Vec<(Vec<usize>, SegmentPaths)> {
    let (segs, outs) = segments(chain, registry);
    segs.iter()
        .map(|s| (outs[s.output_route_indices[0]].clone(), s.paths.clone()))
        .collect()
}

#[test]
fn each_output_runs_the_paths_that_check_it() {
    let chain = chain(
        &["main"],
        vec![mid_output(), effect("pre"), y_split()],
        y_disables(),
    );
    assert_eq!(
        routing(&chain, &registry()),
        vec![
            (vec![0, 1], only("split", &[0])),
            (vec![2, 3], only("split", &[1])),
            (vec![4, 5], only("split", &[0, 1])),
            (vec![6, 7], SegmentPaths::None),
        ],
        "#328: out-a runs path A, out-b path B, out-ab ONE pipeline with both; the mid \
         Output sits before the split and runs no path"
    );
    let (segs, _) = segments(&chain, &registry());
    assert!(
        segs.iter()
            .filter(|s| s.paths != SegmentPaths::None)
            .all(|s| s.block_indices == vec![1, 2]),
        "#328: every Y output runs the shared blocks, then the split the builder shapes"
    );
}

#[test]
fn an_output_no_path_checks_has_no_pipeline() {
    let mut disables = y_disables();
    disables.set_enabled(
        &EndpointNode::PathOutput(leaf("split", 1)),
        r("main", "out-b"),
        false,
    );
    let chain = chain(&["main"], vec![y_split()], disables);
    assert_eq!(
        routing(&chain, &registry()),
        vec![
            (vec![0, 1], only("split", &[0])),
            (vec![4, 5], only("split", &[0, 1]))
        ],
        "#328: out-b is checked on no path — no route, no segment"
    );
}

/// Two E/S on one Y chain: a head input still pairs only with its own E/S's
/// outputs (#716); each pair still runs the paths its output checks.
#[test]
fn a_head_input_still_pairs_only_with_its_own_e_s() {
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![mono("in-main", "dev", 0)],
            outputs: vec![stereo("out-main", "dev", [0, 1])],
        },
        IoBinding {
            id: "second".into(),
            name: "SECOND".into(),
            inputs: vec![mono("in-2", "dev", 1)],
            outputs: vec![stereo("out-2", "dev", [2, 3])],
        },
    ];
    let disables = leaf_disables(
        "split",
        &[&[r("second", "out-2")], &[r("main", "out-main")]],
    );
    let chain = chain(&["main", "second"], vec![y_split()], disables);
    let (segs, outs) = segments(&chain, &registry);
    let pairing: Vec<(usize, Vec<usize>, SegmentPaths)> = segs
        .iter()
        .map(|s| {
            (
                s.entry_group,
                outs[s.output_route_indices[0]].clone(),
                s.paths.clone(),
            )
        })
        .collect();
    assert_eq!(
        pairing,
        vec![
            (0, vec![0, 1], only("split", &[0])),
            (1, vec![2, 3], only("split", &[1]))
        ],
        "#716: MAIN's input feeds only MAIN's output, SECOND's only SECOND's; #328: \
         each through the path its output checks"
    );
}

/// An insert in the shared blocks: the return feeds the tail. With a Y split
/// its outputs run different paths, so the return feeds one pipeline per path
/// set — and a mid Output after the insert rides only ONE of them, or its
/// route would be written twice (doubled level).
#[test]
fn behind_an_insert_the_return_feeds_one_pipeline_per_path_set() {
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![mono("in", "dev", 0)],
            outputs: vec![
                stereo("out-a", "dev", [0, 1]),
                stereo("out-b", "dev", [2, 3]),
            ],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![mono("ret", "dev", 4)],
            outputs: vec![mono("snd", "dev", 4)],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![stereo("aux-out", "aux", [6, 7])],
        },
    ];
    let insert = AudioBlock {
        id: BlockId("loop".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(project::block::InsertBlock {
            model: "standard".into(),
            io: "fx".into(),
        }),
    };
    let disables = leaf_disables("split", &[&[r("main", "out-b")], &[r("main", "out-a")]]);
    let chain = chain(&["main"], vec![insert, mid_output(), y_split()], disables);
    let (segs, outs) = segments(&chain, &registry);

    let finals: Vec<(Vec<Vec<usize>>, SegmentPaths)> = segs
        .iter()
        .filter(|s| s.block_indices == vec![2])
        .map(|s| {
            (
                s.output_route_indices
                    .iter()
                    .map(|&r| outs[r].clone())
                    .collect(),
                s.paths.clone(),
            )
        })
        .collect();
    assert_eq!(
        finals,
        vec![
            (vec![vec![0, 1]], only("split", &[0])),
            (vec![vec![2, 3]], only("split", &[1])),
        ],
        "#328: the return feeds one pipeline per path set — out-a through A, out-b through B"
    );
    let tap_writers = segs
        .iter()
        .filter(|s| {
            s.mid_output_taps
                .iter()
                .any(|t| outs[t.route_idx] == vec![6, 7])
        })
        .count();
    assert_eq!(
        tap_writers, 1,
        "#328: the mid Output after the insert is written by ONE pipeline — two would double it on its route"
    );
}

/// The owner's rig: a Split → Mix of two amps, shared blocks, then a Y whose
/// path A (a cab) feeds out-a and whose empty path B feeds out-b. Each Y
/// output is its own pipeline and runs the whole Mix section before its own
/// Y path — the Mix is never shared between the two outputs' pipelines.
#[test]
fn a_mix_then_a_y_runs_the_mix_in_every_y_output() {
    let disables = leaf_disables(
        "y",
        &[
            &[r("main", "out-b"), r("main", "out-ab")],
            &[r("main", "out-a"), r("main", "out-ab")],
        ],
    );
    let chain = chain(
        &["main"],
        vec![mix_split(), effect("drive"), cab_or_nothing_y()],
        disables,
    );
    assert_eq!(
        routing(&chain, &registry()),
        vec![(vec![0, 1], only("y", &[0])), (vec![2, 3], only("y", &[1]))],
        "#328: behind a Mix, the Y still routes out-a through path A and out-b through path B"
    );
    let (segs, _) = segments(&chain, &registry());
    let blocks: Vec<Vec<usize>> = segs.iter().map(|s| s.block_indices.clone()).collect();
    assert_eq!(
        blocks,
        vec![vec![0, 1, 2], vec![0, 1, 2]],
        "#328: every Y output runs the Mix, the shared blocks and the Y"
    );
}
