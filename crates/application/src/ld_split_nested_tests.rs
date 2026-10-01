//! #328 (spec §10) — no split limit: a split nests inside a path, and every
//! command reaches a block or a split at any depth.

use project::block::{PathRef, PathSide, SplitBlock, SplitEnd};
use serde_json::json;

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

/// `pre → outer Mix (A: [a_0, inner Mix (A: deep | B: —)] | B: b_0) → post`.
fn nested_chain() -> Vec<AudioBlock> {
    vec![
        make_core_block("pre", true),
        split(
            "outer",
            SplitEnd::Mix,
            vec![
                make_core_block("a_0", true),
                split(
                    "inner",
                    SplitEnd::Mix,
                    vec![make_core_block("deep", true)],
                    vec![],
                ),
            ],
            vec![make_core_block("b_0", true)],
        ),
        make_core_block("post", true),
    ]
}

fn outer(project: &Project) -> SplitBlock {
    split_by_id(project, "outer")
}

fn inner(project: &Project) -> SplitBlock {
    let outer = outer(project);
    outer
        .a
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) if b.id.0 == "inner" => Some(split.clone()),
            _ => None,
        })
        .expect("path A of 'outer' holds 'inner'")
}

#[test]
fn add_split_goes_inside_a_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({
            "chain": CHAIN, "position": 1, "end": "mix",
            "path": { "split": "split_0", "side": "a" }
        }),
    )
    .expect("a split nests inside a path");

    let project = project.borrow();
    assert_eq!(
        project.chains[0].blocks.len(),
        3,
        "the top level is unchanged"
    );
    let a = &split_of(&project).a;
    assert_eq!(a.len(), 2);
    assert!(matches!(a[1].kind, AudioBlockKind::Split(_)), "{a:?}");
}

#[test]
fn add_block_reaches_a_nested_path() {
    let project = project_with(nested_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let block = serde_json::to_value(make_core_block("new", true)).expect("serializes");

    dispatch_json(
        &dispatcher,
        "InsertPrebuiltBlock",
        json!({
            "chain": CHAIN, "block": block, "position": 0,
            "path": { "split": "inner", "side": "b" }
        }),
    )
    .expect("a nested path is addressable by its split's id");

    assert_eq!(ids(&inner(&project.borrow()).b), vec!["new"]);
}

#[test]
fn remove_block_reaches_a_nested_path() {
    let project = project_with(nested_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "RemoveBlock",
        json!({ "chain": CHAIN, "block": "deep" }),
    )
    .expect("a block in a nested path is removable");

    assert!(inner(&project.borrow()).a.is_empty());
}

#[test]
fn move_block_lifts_a_nested_block_to_the_top_level() {
    let project = project_with(nested_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({ "chain": CHAIN, "block": "deep", "new_position": 0 }),
    )
    .expect("a nested block moves out");

    let project = project.borrow();
    assert_eq!(project.chains[0].blocks[0].id.0, "deep");
    assert!(inner(&project).a.is_empty());
}

#[test]
fn set_split_end_reaches_a_nested_split() {
    let project = project_with(nested_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "SetSplitEnd",
        json!({ "chain": CHAIN, "split_id": "inner", "end": "y" }),
    )
    .expect("the nested split ends path A of 'outer', so it may be a Y");

    assert_eq!(inner(&project.borrow()).end, SplitEnd::Y);
}

#[test]
fn remove_split_reaches_a_nested_split() {
    let project = project_with(nested_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "RemoveSplit",
        json!({ "chain": CHAIN, "split_id": "inner" }),
    )
    .expect("a nested split is removable");

    assert_eq!(
        ids(&outer(&project.borrow()).a),
        vec!["a_0", "deep"],
        "its path A takes its place inside the parent path"
    );
}

#[test]
fn a_y_in_a_path_refuses_a_block_after_it_in_that_path() {
    let project = project_with(nested_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();
    let path = PathRef {
        split: BlockId("outer".into()),
        side: PathSide::A,
    };

    let err = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 0, "end": "y", "path": path }),
    )
    .expect_err("a_0 would follow the Y in path A");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}
