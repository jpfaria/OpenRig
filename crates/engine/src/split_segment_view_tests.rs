//! #328 — a Y split, as one segment builds it (spec §4.2, §11.3).

use std::borrow::Cow;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::split_param_keys::{level_to, mix_level, mix_pan, mix_polarity};
use project::block::split_params::{default_split_params, MIX_MASTER, MIX_MASTER_SUM};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, PathRef, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::EndpointDisables;
use project::param::ParameterSet;

use super::{block_for_segment, chain_for_segment};
use crate::segment_types::SegmentPaths;

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

fn split_named(
    id: &str,
    end: SplitEnd,
    params: ParameterSet,
    paths: Vec<Vec<AudioBlock>>,
) -> AudioBlock {
    let mut split = SplitBlock::with_paths(end, paths);
    split.params = params;
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(split),
    }
}

fn split(end: SplitEnd, params: ParameterSet) -> AudioBlock {
    split_named(
        "split",
        end,
        params,
        vec![vec![effect("amp-a")], vec![effect("amp-b")]],
    )
}

/// A user's mixer settings on `count` paths — everything a Y output must
/// ignore.
fn user_params(count: usize) -> ParameterSet {
    let mut params = default_split_params(count);
    params.insert(&level_to(0), ParameterValue::Float(40.0));
    for i in 0..count {
        params.insert(&mix_level(i), ParameterValue::Float(10.0 * (i + 1) as f32));
        params.insert(
            &mix_pan(i),
            ParameterValue::Float(if i % 2 == 0 { -50.0 } else { 50.0 }),
        );
        params.insert(&mix_polarity(i), ParameterValue::String("invert".into()));
    }
    params.insert(MIX_MASTER, ParameterValue::Float(5.0));
    params.insert(MIX_MASTER_SUM, ParameterValue::Bool(true));
    params
}

fn leaf(split: &str, path: usize) -> PathRef {
    PathRef {
        split: BlockId(split.into()),
        path,
    }
}

fn only(split: &str, paths: &[usize]) -> SegmentPaths {
    SegmentPaths::Only(paths.iter().map(|&p| leaf(split, p)).collect())
}

fn view(block: &AudioBlock, paths: &SegmentPaths) -> SplitBlock {
    match block_for_segment(block, paths).into_owned().kind {
        AudioBlockKind::Split(split) => split,
        _ => panic!("a split must stay a split"),
    }
}

fn ids(blocks: &[AudioBlock]) -> Vec<&str> {
    blocks.iter().map(|b| b.id.0.as_str()).collect()
}

fn mix_levels(shaped: &SplitBlock) -> Vec<Option<f32>> {
    (0..shaped.paths.len())
        .map(|i| shaped.params.get_f32(&mix_level(i)))
        .collect()
}

fn assert_neutral_knobs(shaped: &SplitBlock) {
    let defaults = default_split_params(shaped.paths.len());
    let mut keys = vec![MIX_MASTER_SUM.to_string()];
    for i in 0..shaped.paths.len() {
        keys.push(mix_pan(i));
        keys.push(mix_polarity(i));
    }
    for key in &keys {
        assert_eq!(
            shaped.params.get(key),
            defaults.get(key),
            "#328: {key} takes its neutral default — Y has no mixer"
        );
    }
    assert_eq!(
        shaped.params.get_f32(MIX_MASTER),
        Some(100.0),
        "#328: master at unity (its default 50 would halve the output)"
    );
}

#[test]
fn a_y_output_fed_by_path_a_runs_path_a_alone_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params(2)), &only("split", &[0]));
    assert!(
        matches!(shaped.end, SplitEnd::Mix),
        "#328: the segment runs the split through the Split → Mix code"
    );
    assert_eq!(ids(&shaped.paths[0]), vec!["amp-a"]);
    assert!(
        shaped.paths[1].is_empty(),
        "#328: path B does not feed this output — it is not built"
    );
    assert_eq!(
        mix_levels(&shaped),
        vec![Some(100.0), Some(0.0)],
        "#328: path A at unity; the empty path B contributes nothing — not even the dry split input"
    );
    assert_neutral_knobs(&shaped);
    assert_eq!(
        shaped.params.get_f32(&level_to(0)),
        Some(40.0),
        "the split's own knobs still apply"
    );
}

#[test]
fn a_y_output_fed_by_path_b_runs_path_b_alone_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params(2)), &only("split", &[1]));
    assert!(matches!(shaped.end, SplitEnd::Mix));
    assert!(
        shaped.paths[0].is_empty(),
        "#328: path A does not feed this output — it is not built"
    );
    assert_eq!(ids(&shaped.paths[1]), vec!["amp-b"]);
    assert_eq!(mix_levels(&shaped), vec![Some(0.0), Some(100.0)]);
    assert_neutral_knobs(&shaped);
}

#[test]
fn a_y_output_fed_by_both_paths_sums_them_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params(2)), &only("split", &[0, 1]));
    assert_eq!(
        (ids(&shaped.paths[0]), ids(&shaped.paths[1])),
        (vec!["amp-a"], vec!["amp-b"])
    );
    assert_eq!(
        mix_levels(&shaped),
        vec![Some(100.0), Some(100.0)],
        "#328: A + B at unity, aligned by the Split → Mix code"
    );
    assert_neutral_knobs(&shaped);
}

#[test]
fn a_three_path_y_runs_only_the_leaves_its_output_checks() {
    let block = split_named(
        "split",
        SplitEnd::Y,
        user_params(3),
        vec![
            vec![effect("amp-a")],
            vec![effect("amp-b")],
            vec![effect("amp-c")],
        ],
    );
    let shaped = view(&block, &only("split", &[0, 2]));
    assert_eq!(ids(&shaped.paths[0]), vec!["amp-a"]);
    assert!(
        shaped.paths[1].is_empty(),
        "#328: leaf B feeds another output"
    );
    assert_eq!(ids(&shaped.paths[2]), vec!["amp-c"]);
    assert_eq!(
        mix_levels(&shaped),
        vec![Some(100.0), Some(0.0), Some(100.0)],
        "#328 §11.3: A + C at unity, B silent"
    );
    assert_neutral_knobs(&shaped);
}

#[test]
fn a_y_nested_in_a_path_is_reached_through_its_parent() {
    let inner = split_named(
        "inner",
        SplitEnd::Y,
        user_params(2),
        vec![vec![effect("cab-1")], vec![effect("cab-2")]],
    );
    let outer = split_named(
        "outer",
        SplitEnd::Y,
        user_params(2),
        vec![vec![effect("amp-a"), inner], vec![effect("amp-b")]],
    );
    let shaped = view(&outer, &only("inner", &[1]));
    assert_eq!(
        mix_levels(&shaped),
        vec![Some(100.0), Some(0.0)],
        "#328 §11.3: the outer path that holds the inner Y runs; the other does not"
    );
    assert_eq!(ids(&shaped.paths[0]), vec!["amp-a", "inner"]);
    let AudioBlockKind::Split(inner) = &shaped.paths[0][1].kind else {
        panic!("the inner split stays a split");
    };
    assert!(matches!(inner.end, SplitEnd::Mix));
    assert!(
        inner.paths[0].is_empty(),
        "inner leaf A feeds another output"
    );
    assert_eq!(ids(&inner.paths[1]), vec!["cab-2"]);
    assert_eq!(mix_levels(inner), vec![Some(0.0), Some(100.0)]);
}

#[test]
fn a_render_with_no_routing_plays_every_path() {
    let block = split(SplitEnd::Y, user_params(2));
    assert_eq!(
        view(&block, &SegmentPaths::None),
        view(&block, &only("split", &[0, 1])),
        "#328: an offline render hears the chain as an output with every leaf checked"
    );
}

#[test]
fn a_mix_split_and_every_other_block_are_themselves() {
    let mix = split(SplitEnd::Mix, user_params(2));
    assert!(
        matches!(
            block_for_segment(&mix, &only("split", &[0])),
            Cow::Borrowed(_)
        ),
        "#328: a Split → Mix keeps its own mixer"
    );
    let amp = effect("amp");
    assert!(matches!(
        block_for_segment(&amp, &only("split", &[0, 1])),
        Cow::Borrowed(_)
    ));
}

#[test]
fn a_chain_is_shaped_only_where_its_y_split_sits() {
    let chain = |blocks: Vec<AudioBlock>| Chain {
        mix: Default::default(),
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables::default(),
    };
    let linear = chain(vec![effect("pre")]);
    assert!(
        matches!(
            chain_for_segment(&linear, &only("split", &[0])),
            Cow::Borrowed(_)
        ),
        "a chain with no Y split is itself — nothing is cloned"
    );
    let y = chain(vec![effect("pre"), split(SplitEnd::Y, user_params(2))]);
    let paths = only("split", &[1]);
    let shaped = chain_for_segment(&y, &paths);
    assert!(
        matches!(shaped, Cow::Owned(_)),
        "#328: a chain with a Y split is shaped for each segment"
    );
    assert_eq!(shaped.blocks.len(), 2);
    assert_eq!(
        shaped.blocks[0], y.blocks[0],
        "the shared blocks are untouched"
    );
    assert_eq!(
        shaped.blocks[1],
        block_for_segment(&y.blocks[1], &paths).into_owned(),
        "#328: the split is shaped for the segment"
    );
}
