//! #328 — a Y → A/B split, as one segment builds it.

use std::borrow::Cow;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, LEVEL_TO_A, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER,
    MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
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

fn split(end: SplitEnd, params: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params,
            a: vec![effect("amp-a")],
            b: vec![effect("amp-b")],
        }),
    }
}

/// A user's mixer settings — everything a Y → A/B output must ignore.
fn user_params() -> ParameterSet {
    let mut params = default_split_params();
    params.insert(LEVEL_TO_A, ParameterValue::Float(40.0));
    params.insert(MIX_LEVEL_A, ParameterValue::Float(10.0));
    params.insert(MIX_LEVEL_B, ParameterValue::Float(20.0));
    params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    params.insert(MIX_PAN_B, ParameterValue::Float(50.0));
    params.insert(MIX_B_POLARITY, ParameterValue::String("invert".into()));
    params.insert(MIX_MASTER, ParameterValue::Float(5.0));
    params.insert(MIX_MASTER_SUM, ParameterValue::Bool(true));
    params
}

fn view(block: &AudioBlock, paths: SegmentPaths) -> SplitBlock {
    match block_for_segment(block, paths).into_owned().kind {
        AudioBlockKind::Split(split) => split,
        _ => panic!("a split must stay a split"),
    }
}

fn ids(blocks: &[AudioBlock]) -> Vec<&str> {
    blocks.iter().map(|b| b.id.0.as_str()).collect()
}

fn assert_neutral_knobs(shaped: &SplitBlock) {
    let defaults = default_split_params();
    for key in [MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER_SUM] {
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
    let shaped = view(&split(SplitEnd::Y, user_params()), SegmentPaths::A);
    assert!(
        matches!(shaped.end, SplitEnd::Mix),
        "#328: the segment runs the split through the Split → Mix code"
    );
    assert_eq!(ids(&shaped.a), vec!["amp-a"]);
    assert!(
        shaped.b.is_empty(),
        "#328: path B does not feed this output — it is not built"
    );
    assert_eq!(
        shaped.params.get_f32(MIX_LEVEL_A),
        Some(100.0),
        "path A at unity"
    );
    assert_eq!(
        shaped.params.get_f32(MIX_LEVEL_B),
        Some(0.0),
        "#328: the empty path B contributes nothing — not even the dry split input"
    );
    assert_neutral_knobs(&shaped);
    assert_eq!(
        shaped.params.get_f32(LEVEL_TO_A),
        Some(40.0),
        "the split's own knobs still apply"
    );
}

#[test]
fn a_y_output_fed_by_path_b_runs_path_b_alone_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params()), SegmentPaths::B);
    assert!(matches!(shaped.end, SplitEnd::Mix));
    assert!(
        shaped.a.is_empty(),
        "#328: path A does not feed this output — it is not built"
    );
    assert_eq!(ids(&shaped.b), vec!["amp-b"]);
    assert_eq!(shaped.params.get_f32(MIX_LEVEL_A), Some(0.0));
    assert_eq!(shaped.params.get_f32(MIX_LEVEL_B), Some(100.0));
    assert_neutral_knobs(&shaped);
}

#[test]
fn a_y_output_fed_by_both_paths_sums_them_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params()), SegmentPaths::AB);
    assert_eq!(
        (ids(&shaped.a), ids(&shaped.b)),
        (vec!["amp-a"], vec!["amp-b"])
    );
    assert_eq!(
        (
            shaped.params.get_f32(MIX_LEVEL_A),
            shaped.params.get_f32(MIX_LEVEL_B)
        ),
        (Some(100.0), Some(100.0)),
        "#328: A + B at unity, aligned by the Split → Mix code"
    );
    assert_neutral_knobs(&shaped);
}

#[test]
fn a_render_with_no_routing_plays_both_paths() {
    let block = split(SplitEnd::Y, user_params());
    assert_eq!(
        view(&block, SegmentPaths::None),
        view(&block, SegmentPaths::AB),
        "#328: an offline render hears the chain as an output with both paths checked"
    );
}

#[test]
fn a_mix_split_and_every_other_block_are_themselves() {
    let mix = split(SplitEnd::Mix, user_params());
    assert!(
        matches!(block_for_segment(&mix, SegmentPaths::A), Cow::Borrowed(_)),
        "#328: a Split → Mix keeps its own mixer"
    );
    let amp = effect("amp");
    assert!(matches!(
        block_for_segment(&amp, SegmentPaths::AB),
        Cow::Borrowed(_)
    ));
}

#[test]
fn a_chain_is_shaped_only_where_its_y_split_sits() {
    let chain = |blocks: Vec<AudioBlock>| Chain {
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
            chain_for_segment(&linear, SegmentPaths::A),
            Cow::Borrowed(_)
        ),
        "a chain with no Y split is itself — nothing is cloned"
    );
    let y = chain(vec![effect("pre"), split(SplitEnd::Y, user_params())]);
    let shaped = chain_for_segment(&y, SegmentPaths::B);
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
        block_for_segment(&y.blocks[1], SegmentPaths::B).into_owned(),
        "#328: the split is shaped for the segment"
    );
}
