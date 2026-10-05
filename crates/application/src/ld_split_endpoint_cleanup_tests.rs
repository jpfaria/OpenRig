//! #328 (spec §1.3, §11.3) — an edit that takes Y leaves away forgets their
//! output checklists, and only theirs.
//!
//! The checklists belong to the rig input, not to one preset: a leaf of a Y
//! the chain is not playing right now (another preset's) must survive any
//! edit of this preset.

use project::block::{PathRef, SplitBlock, SplitEnd};
use project::endpoint_disables::{EndpointNode, EndpointRef};
use serde_json::json;

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

fn out(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "io".into(),
        endpoint: endpoint.into(),
    }
}

fn leaf(split: &str, path: usize) -> EndpointNode {
    EndpointNode::PathOutput(PathRef {
        split: BlockId(split.into()),
        path,
    })
}

/// Uncheck `out L` on each leaf of `leaves`.
fn uncheck(project: &Rc<RefCell<Project>>, leaves: &[(&str, usize)]) {
    let mut project = project.borrow_mut();
    for (split, path) in leaves {
        project.chains[0]
            .disabled_endpoints
            .set_enabled(&leaf(split, *path), out("out L"), false);
    }
}

/// The leaves that still hold an unchecked endpoint.
fn remembered(project: &Rc<RefCell<Project>>) -> Vec<(String, usize)> {
    project.borrow().chains[0]
        .disabled_endpoints
        .path_outputs
        .iter()
        .map(|entry| (entry.split.0.clone(), entry.path))
        .collect()
}

/// `pre → split_0 Y (A: a_0 | B: inner Y (A: deep | B: —) | C: c_0)`.
fn three_path_y() -> Vec<AudioBlock> {
    let inner = split(
        "inner",
        SplitEnd::Y,
        vec![make_core_block("deep", true)],
        vec![],
    );
    vec![
        make_core_block("pre", true),
        AudioBlock {
            id: BlockId("split_0".into()),
            enabled: true,
            kind: AudioBlockKind::Split(SplitBlock::with_paths(
                SplitEnd::Y,
                vec![
                    vec![make_core_block("a_0", true)],
                    vec![inner],
                    vec![make_core_block("c_0", true)],
                ],
            )),
        },
    ]
}

#[test]
fn remove_split_forgets_its_leaves_and_keeps_another_presets() {
    let project = project_with(y_chain());
    uncheck(&project, &[("split_0", 1), ("other_preset_y", 0)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "RemoveSplit",
        json!({ "chain": CHAIN, "split_id": "split_0" }),
    )
    .expect("the Y is removable");

    assert_eq!(
        remembered(&project),
        vec![("other_preset_y".to_string(), 0)]
    );
}

#[test]
fn a_y_turned_into_a_mix_forgets_its_leaves() {
    let project = project_with(y_chain());
    uncheck(&project, &[("split_0", 0), ("other_preset_y", 1)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "SetSplitEnd",
        json!({ "chain": CHAIN, "split_id": "split_0", "end": "mix" }),
    )
    .expect("a Y becomes a Mix");

    assert_eq!(
        remembered(&project),
        vec![("other_preset_y".to_string(), 1)]
    );
}

#[test]
fn remove_split_path_keeps_another_presets_leaves() {
    let project = project_with(three_path_y());
    uncheck(
        &project,
        &[("split_0", 2), ("inner", 0), ("other_preset_y", 0)],
    );
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "RemoveSplitPath",
        json!({ "chain": CHAIN, "split_id": "split_0", "path": 1 }),
    )
    .expect("path B goes, with the Y nested in it");

    let mut left = remembered(&project);
    left.sort();
    assert_eq!(
        left,
        vec![
            ("other_preset_y".to_string(), 0),
            ("split_0".to_string(), 1),
        ],
        "path C became path B; the nested Y went with path B"
    );
}
