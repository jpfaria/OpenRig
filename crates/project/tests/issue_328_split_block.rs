//! #328 — `AudioBlockKind::Split` (spec §1.1): the split sits in a chain's
//! block list like any block, carries two paths, and its knobs go through the
//! ordinary parameter pipeline.

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::param_writer::{set_parameter_number, set_parameter_option};
use project::block::split_params::{MIX_PAN_A, SPLIT_MODE};
use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::param::ParameterSet;

fn delay(id: &str) -> AudioBlock {
    let model = block_delay::supported_models()
        .first()
        .expect("a native delay model")
        .to_string();
    let schema = schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    }
}

fn split(end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            b,
            ..SplitBlock::new(end)
        }),
    }
}

fn split_of(block: &AudioBlock) -> &SplitBlock {
    match &block.kind {
        AudioBlockKind::Split(s) => s,
        other => panic!("expected a split, got {}", other.label()),
    }
}

#[test]
fn a_split_reports_its_kind_and_only_a_y_is_routing() {
    let mix = split(SplitEnd::Mix, vec![], vec![]);
    assert_eq!(mix.kind.label(), "split");
    assert!(
        !mix.kind.is_routing(),
        "a Split → Mix is DSP inside one segment — toggling it never reopens streams"
    );
    assert!(mix.model_ref().is_none(), "a split has no single model");

    let y = split(SplitEnd::Y, vec![], vec![]);
    assert!(
        y.kind.is_routing(),
        "a Y → A/B decides which streams exist — toggling it is a rebuild"
    );
}

#[test]
fn identity_follows_the_paths_structure_but_not_their_knobs() {
    let base = split(SplitEnd::Mix, vec![delay("amp-a")], vec![delay("amp-b")]);
    let identity = |b: &AudioBlock| b.kind.model_identity();

    let mut knob = base.clone();
    if let AudioBlockKind::Split(s) = &mut knob.kind {
        s.a[0].enabled = false;
        s.params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    }
    assert_eq!(
        identity(&knob),
        identity(&base),
        "a knob or a bypass is scene state, not structure"
    );

    let added = split(
        SplitEnd::Mix,
        vec![delay("amp-a"), delay("comp")],
        vec![delay("amp-b")],
    );
    assert_ne!(
        identity(&added),
        identity(&base),
        "a block added inside path A is structural"
    );

    let moved = split(SplitEnd::Mix, vec![], vec![delay("amp-a"), delay("amp-b")]);
    assert_ne!(
        identity(&moved),
        identity(&base),
        "a block moved from A to B is structural"
    );

    let other_end = split(SplitEnd::Y, vec![delay("amp-a")], vec![delay("amp-b")]);
    assert_ne!(
        identity(&other_end),
        identity(&base),
        "Mix → Y is structural"
    );

    let mut swapped = base.clone();
    if let AudioBlockKind::Split(s) = &mut swapped.kind {
        if let AudioBlockKind::Core(core) = &mut s.b[0].kind {
            core.model = "another_model".into();
        }
    }
    assert_ne!(
        identity(&swapped),
        identity(&base),
        "a model swap inside a path changes the path's identity"
    );
}

#[test]
fn the_parameters_of_a_split_are_its_knobs() {
    let block = split(SplitEnd::Mix, vec![delay("amp-a")], vec![]);
    let descriptors = block.parameter_descriptors().expect("describe");
    assert_eq!(descriptors.len(), 12, "the twelve split and mixer knobs");
    assert_eq!(descriptors[0].id.0, "split::split_mode");
}

#[test]
fn the_audio_of_a_split_is_the_audio_of_its_path_blocks() {
    let block = split(SplitEnd::Mix, vec![delay("amp-a")], vec![delay("amp-b")]);
    let ids: Vec<String> = block
        .audio_descriptors()
        .expect("describe")
        .into_iter()
        .map(|d| d.block_id.0)
        .collect();
    assert_eq!(ids, vec!["amp-a", "amp-b"]);
}

#[test]
fn validate_params_checks_the_knobs_and_the_path_blocks() {
    assert!(
        split(SplitEnd::Mix, vec![delay("amp-a")], vec![delay("amp-b")])
            .validate_params()
            .is_ok()
    );
    let mut out_of_range = split(SplitEnd::Mix, vec![], vec![]);
    if let AudioBlockKind::Split(s) = &mut out_of_range.kind {
        s.params.insert(MIX_PAN_A, ParameterValue::Float(80.0));
    }
    let err = out_of_range
        .validate_params()
        .expect_err("80 is outside −50…+50");
    assert!(
        err.contains(MIX_PAN_A),
        "the error names the knob, got: {err}"
    );
}

#[test]
fn the_knobs_are_edited_through_the_ordinary_parameter_writers() {
    let mut block = split(SplitEnd::Mix, vec![], vec![]);
    set_parameter_number(&mut block, MIX_PAN_A, -50.0).expect("a split knob is writable");
    set_parameter_option(&mut block, SPLIT_MODE, "dual_mono").expect("an option knob too");
    let s = split_of(&block);
    assert_eq!(s.params.get_f32(MIX_PAN_A), Some(-50.0));
    assert_eq!(s.params.get_string(SPLIT_MODE), Some("dual_mono"));
}
