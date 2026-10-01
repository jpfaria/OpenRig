//! #328 — the read side reaches the blocks inside a split's paths, so an MCP
//! client (`openrig://ids`, the block-params resource) sees what the GUI shows.

use domain::ids::{BlockId, ChainId};
use project::block::SplitEnd;

use crate::block_factory::build_default_block;
use crate::query::{get_block_params, list_ids};
use crate::split_tests_fixtures::{mix_chain, project_with, split, CHAIN};

#[test]
fn get_block_params_reaches_a_block_inside_a_split_path() {
    let fuzz = build_default_block(BlockId("a_fuzz".into()), "gain", "fuzz_ge")
        .expect("fuzz_ge is a shipped gain model");
    let project = project_with(vec![split("split_0", SplitEnd::Mix, vec![fuzz], vec![])]);

    let json = get_block_params(
        &project.borrow(),
        &ChainId(CHAIN.into()),
        &BlockId("a_fuzz".into()),
    )
    .expect("a block inside path A is readable by id");

    assert!(json.starts_with("{\"params\":"), "{json}");
}

#[test]
fn list_ids_lists_the_blocks_inside_each_split_path() {
    let project = project_with(mix_chain());

    let out = list_ids(&project.borrow());

    assert!(out.contains("  block split_0  split  enabled"), "{out}");
    assert!(
        out.contains("    path A  block a_0  core  enabled"),
        "{out}"
    );
    assert!(
        out.contains("    path B  block b_0  core  enabled"),
        "{out}"
    );
}
