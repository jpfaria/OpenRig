//! #328 — the structural rules of a chain split (spec §1.1): at most one split
//! per chain, a path holds processing blocks only (no split, select or port,
//! so nesting stays one level deep), and a Y split ends the chain.

use std::collections::BTreeMap;

use domain::ids::BlockId;
use project::block::{
    find_split, schema_for_block_model, validate_split_layout, AudioBlock, AudioBlockKind,
    CoreBlock, InputBlock, InsertBlock, OutputBlock, SelectBlock, SplitBlock, SplitEnd,
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
        AudioBlockKind::Split(SplitBlock {
            a,
            ..SplitBlock::new(end)
        }),
    )
}

fn rig(blocks: Vec<AudioBlock>) -> RigProject {
    RigProject {
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
        (split("inner", SplitEnd::Mix, vec![]), "split"),
        (select("sel"), "select"),
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
fn a_chain_holds_at_most_one_split() {
    let blocks = vec![
        split("s1", SplitEnd::Mix, vec![]),
        delay("amp"),
        split("s2", SplitEnd::Mix, vec![]),
    ];
    let err = validate_split_layout(&blocks).expect_err("two splits");
    assert!(err.contains("at most one split"), "got: {err}");
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
    assert_eq!(found.a[0].id.0, "amp");
    assert!(find_split(&[delay("drive")]).is_none());
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
        split("s1", SplitEnd::Mix, vec![]),
        split("s2", SplitEnd::Mix, vec![]),
    ])
    .validate()
    .expect_err("two splits");
    assert!(
        err.contains("preset 'p'") && err.contains("at most one split"),
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
