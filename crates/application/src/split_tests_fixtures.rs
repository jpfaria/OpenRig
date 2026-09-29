//! Shared fixtures for the #328 split command tests.
//!
//! One chain (`CHAIN`) whose blocks each test sets. Later tasks add a
//! one-input rig for the tests that follow an edit into `project.openrig`.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::BlockId;
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use project::project::Project;

use crate::local_dispatcher_tests::{make_core_block, make_project};

/// The chain every non-rig split test edits.
pub(crate) const CHAIN: &str = "chain_0";

/// A split block carrying the default knobs of spec §1.2.
pub(crate) fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params: default_split_params(),
            a,
            b,
        }),
    }
}

/// `[pre, split_0 → Mix { a: [a_0], b: [b_0] }, post]`.
pub(crate) fn mix_chain() -> Vec<AudioBlock> {
    vec![
        make_core_block("pre", true),
        split(
            "split_0",
            SplitEnd::Mix,
            vec![make_core_block("a_0", true)],
            vec![make_core_block("b_0", true)],
        ),
        make_core_block("post", true),
    ]
}

/// A project whose only chain, `CHAIN`, holds exactly `blocks`.
pub(crate) fn project_with(blocks: Vec<AudioBlock>) -> Rc<RefCell<Project>> {
    let project = make_project(CHAIN, make_core_block("seed", true));
    project.borrow_mut().chains[0].blocks = blocks;
    project
}

/// The split of the project's first chain (panics when it has none).
pub(crate) fn split_of(project: &Project) -> SplitBlock {
    project.chains[0]
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) => Some(split.clone()),
            _ => None,
        })
        .expect("the first chain holds a split")
}

/// The ids of `blocks`, in order.
pub(crate) fn ids(blocks: &[AudioBlock]) -> Vec<String> {
    blocks.iter().map(|b| b.id.0.clone()).collect()
}
