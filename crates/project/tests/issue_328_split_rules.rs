//! #328 — the structural rules of a chain split (spec §10.1): any number of
//! splits, nested to any depth, and only two rules left — a Y ends the list it
//! sits in (the chain's own ports excepted at the top level), and a path holds
//! no port, insert or select.

use std::collections::BTreeMap;

use domain::ids::BlockId;
use project::block::{
    find_split, find_split_with_end, has_y_split, schema_for_block_model, splits,
    validate_split_layout, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InsertBlock,
    OutputBlock, SelectBlock, SplitBlock, SplitEnd,
};
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};

fn block(id: &str, kind: AudioBlockKind) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind,
    }
}

fn delay(id: &str) -> AudioBlock {
    let model = block_delay::supported_models()
        .first()
        .expect("a native delay model")
        .to_string();
    let schema = schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults");
    block(
        id,
        AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    )
}

fn input_port(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "In".into(),
        }),
    )
}

fn output_port(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Out".into(),
        }),
    )
}

fn insert(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    )
}

fn select(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId(format!("{id}::opt")),
            options: vec![delay(&format!("{id}::opt"))],
        }),
    )
}

fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Split(SplitBlock::with_paths(end, vec![a, Vec::new()])),
    )
}

fn rig(blocks: Vec<AudioBlock>) -> RigProject {
    RigProject {
        bpm: None,
        name: None,
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                mix: Default::default(),
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
                disabled_endpoints: Default::default(),
                di_output: None,
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "p".to_string(),
            RigPreset::from_legacy_blocks(blocks, 100.0),
        )]),
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn a_path_holds_processing_blocks_only() {
    let forbidden = [
        (input_port("in"), "input"),
        (output_port("out"), "output"),
        (insert("fx"), "insert"),
    ];
    for (bad, label) in forbidden {
        let outer = split("outer", SplitEnd::Mix, vec![delay("ok"), bad]);
        let AudioBlockKind::Split(s) = &outer.kind else {
            unreachable!()
        };
        let err = s
            .validate_structure()
            .expect_err("a forbidden block in a path");
        // `is a <label> block`, not just `<label>`: every message starts with
        // "split path block", so a bare `contains("split")` would always pass.
        assert!(
            err.contains(&format!("is a {label} block")),
            "the error names the {label} block, got: {err}"
        );
        assert!(
            outer.validate_params().is_err(),
            "validate_params enforces it too ({label})"
        );
    }
    let ok = split("s", SplitEnd::Mix, vec![delay("amp")]);
    let AudioBlockKind::Split(s) = &ok.kind else {
        unreachable!()
    };
    assert!(s.validate_structure().is_ok());
}

#[test]
fn a_chain_holds_any_number_of_splits() {
    assert!(
        validate_split_layout(&[
            split("s1", SplitEnd::Mix, vec![delay("a1")]),
            delay("amp"),
            split("s2", SplitEnd::Mix, vec![delay("a2")]),
            split("s3", SplitEnd::Mix, vec![delay("a3")]),
        ])
        .is_ok(),
        "the limit is the machine, not a count (spec §10.1)"
    );
}

#[test]
fn a_split_nests_inside_a_path() {
    let inner = split("inner", SplitEnd::Mix, vec![delay("deep")]);
    let outer = split("outer", SplitEnd::Mix, vec![delay("amp"), inner]);
    let AudioBlockKind::Split(s) = &outer.kind else {
        unreachable!()
    };
    assert!(s.validate_structure().is_ok(), "nesting has no depth limit");
    assert!(outer.validate_params().is_ok());
    assert!(validate_split_layout(&[outer]).is_ok());
}

#[test]
fn the_rules_are_checked_at_every_depth() {
    let bad_inner = split("inner", SplitEnd::Mix, vec![input_port("in")]);
    let outer = split("outer", SplitEnd::Mix, vec![bad_inner]);
    let err = validate_split_layout(&[outer]).expect_err("a port in a nested path");
    assert!(err.contains("is a input block"), "got: {err}");
}

#[test]
fn a_y_ends_the_path_it_sits_in() {
    let bad = split(
        "outer",
        SplitEnd::Mix,
        vec![split("y", SplitEnd::Y, vec![delay("cab")]), delay("reverb")],
    );
    let err = validate_split_layout(&[bad]).expect_err("a block after a nested Y");
    assert!(
        err.contains("Y split") && err.contains("reverb"),
        "got: {err}"
    );
    let ok = split(
        "outer",
        SplitEnd::Mix,
        vec![delay("amp"), split("y", SplitEnd::Y, vec![delay("cab")])],
    );
    assert!(
        validate_split_layout(&[ok, delay("post")]).is_ok(),
        "a Y that ends its own path leaves the Mix free to continue"
    );
}

#[test]
fn a_y_split_ends_the_chain() {
    let err = validate_split_layout(&[split("s", SplitEnd::Y, vec![]), delay("reverb")])
        .expect_err("a block after a Y split");
    assert!(
        err.contains("reverb"),
        "the error names the block that follows, got: {err}"
    );
    assert!(
        validate_split_layout(&[
            delay("drive"),
            split("s", SplitEnd::Y, vec![]),
            output_port("tail")
        ])
        .is_ok(),
        "only the chain's own ports may follow a Y split"
    );
    assert!(
        validate_split_layout(&[split("s", SplitEnd::Y, vec![]), insert("fx")]).is_err(),
        "an insert is in the signal path, it cannot follow a Y split"
    );
    assert!(
        validate_split_layout(&[split("s", SplitEnd::Mix, vec![]), delay("reverb")]).is_ok(),
        "a Mix split continues into shared blocks"
    );
}

#[test]
fn find_split_reports_the_split_and_where_it_sits() {
    let blocks = vec![
        delay("drive"),
        split("s", SplitEnd::Mix, vec![delay("amp")]),
    ];
    let (position, found) = find_split(&blocks).expect("the chain has a split");
    assert_eq!(position, 1);
    assert_eq!(found.paths[0][0].id.0, "amp");
    assert!(find_split(&[delay("drive")]).is_none());
}

#[test]
fn the_split_lookups_see_every_split() {
    let blocks = vec![
        delay("drive"),
        split("mix", SplitEnd::Mix, vec![delay("amp1")]),
        delay("delay"),
        split("y", SplitEnd::Y, vec![delay("cab")]),
    ];
    let found: Vec<(usize, SplitEnd)> = splits(&blocks).map(|(p, s)| (p, s.end)).collect();
    assert_eq!(found, vec![(1, SplitEnd::Mix), (3, SplitEnd::Y)]);
    let (position, y) = find_split_with_end(&blocks, SplitEnd::Y).expect("the Y split");
    assert_eq!(position, 3);
    assert_eq!(y.paths[0][0].id.0, "cab");
    assert!(has_y_split(&blocks), "the Y sits behind a Mix");
    assert!(!has_y_split(&blocks[..3]), "a Mix alone is not a Y");
    assert!(find_split_with_end(&blocks[..3], SplitEnd::Y).is_none());
}

#[test]
fn a_select_option_cannot_be_a_split() {
    let select = SelectBlock {
        selected_block_id: BlockId("s".into()),
        options: vec![split("s", SplitEnd::Mix, vec![])],
    };
    let err = select.validate_structure().expect_err("a split option");
    assert!(err.contains("split"), "got: {err}");
}

#[test]
fn a_rig_refuses_a_preset_that_breaks_the_split_rules() {
    let err = rig(vec![
        split("y", SplitEnd::Y, vec![]),
        split("s2", SplitEnd::Mix, vec![]),
    ])
    .validate()
    .expect_err("a split after a Y");
    assert!(
        err.contains("preset 'p'") && err.contains("Y split"),
        "got: {err}"
    );
    let err = rig(vec![split("s", SplitEnd::Mix, vec![input_port("in")])])
        .validate()
        .expect_err("a port in a path");
    assert!(
        err.contains("preset 'p'") && err.contains("is a input block"),
        "got: {err}"
    );
    assert!(rig(vec![
        delay("drive"),
        split("s", SplitEnd::Y, vec![delay("amp")])
    ])
    .validate()
    .is_ok());
}

#[test]
fn a_mix_then_a_y_is_accepted() {
    assert!(
        validate_split_layout(&[
            delay("drive"),
            split("mix", SplitEnd::Mix, vec![delay("amp1")]),
            delay("delay"),
            split("y", SplitEnd::Y, vec![delay("cab")]),
            output_port("tail"),
        ])
        .is_ok(),
        "one Mix, then blocks, then the Y last"
    );
}

#[test]
fn a_block_after_the_y_is_refused_when_a_mix_comes_first() {
    let err = validate_split_layout(&[
        split("mix", SplitEnd::Mix, vec![]),
        split("y", SplitEnd::Y, vec![]),
        delay("reverb"),
    ])
    .expect_err("a block after the Y split");
    assert!(
        err.contains("Y split") && err.contains("reverb"),
        "the Y is judged from its own position, got: {err}"
    );
}

#[test]
fn a_y_before_a_mix_is_refused() {
    let err = validate_split_layout(&[
        split("y", SplitEnd::Y, vec![]),
        split("mix", SplitEnd::Mix, vec![]),
    ])
    .expect_err("a Mix after the Y split");
    assert!(
        err.contains("Y split") && err.contains("mix"),
        "the Mix follows the Y, got: {err}"
    );
}

#[test]
fn a_rig_accepts_a_mix_then_a_y() {
    assert!(rig(vec![
        split("mix", SplitEnd::Mix, vec![delay("amp1")]),
        delay("delay"),
        split("y", SplitEnd::Y, vec![delay("cab")]),
    ])
    .validate()
    .is_ok());
}

#[test]
fn a_rig_refuses_a_port_in_the_y_path_behind_a_mix() {
    for (bad, label) in [(input_port("in"), "input"), (output_port("out"), "output")] {
        let err = rig(vec![
            split("mix", SplitEnd::Mix, vec![delay("amp1")]),
            split("y", SplitEnd::Y, vec![bad]),
        ])
        .validate()
        .expect_err("a forbidden block in the Y path");
        assert!(
            err.contains("preset 'p'") && err.contains(&format!("is a {label} block")),
            "every split's paths are checked ({label}), got: {err}"
        );
    }
}

#[test]
fn a_split_holds_any_number_of_paths() {
    let s = SplitBlock::with_paths(
        SplitEnd::Mix,
        vec![
            vec![delay("p0")],
            vec![delay("p1")],
            vec![delay("p2")],
            Vec::new(),
        ],
    );
    assert_eq!(s.paths.len(), 4);
    assert!(s.validate_structure().is_ok());
}

#[test]
fn a_split_refuses_fewer_than_two_paths() {
    let s = SplitBlock::with_paths(SplitEnd::Mix, vec![vec![delay("only")]]);
    let err = s.validate_structure().expect_err("one path");
    assert!(err.contains("at least 2 paths"), "got: {err}");
}

#[test]
fn a_select_may_sit_in_a_path() {
    let s = SplitBlock::with_paths(SplitEnd::Mix, vec![vec![select("sel")], Vec::new()]);
    assert!(
        s.validate_structure().is_ok(),
        "spec §11: Select is allowed in a path"
    );
}

#[test]
fn path_letters_name_any_index() {
    use project::block::path_letter;
    assert_eq!(path_letter(0), "A");
    assert_eq!(path_letter(2), "C");
    assert_eq!(path_letter(25), "Z");
    assert_eq!(path_letter(26), "AA");
}
