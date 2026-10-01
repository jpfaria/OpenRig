//! #328 — a cloned chain gets fresh ids for the blocks inside its split paths.

use super::*;
use crate::chain_graph_fixtures_tests::mix_then_y_chain;

fn all_ids(blocks: &[AudioBlock]) -> Vec<String> {
    let mut ids = Vec::new();
    for block in blocks {
        ids.push(block.id.0.clone());
        if let AudioBlockKind::Split(split) = &block.kind {
            for path in &split.paths {
                ids.extend(all_ids(path));
            }
        }
    }
    ids
}

#[test]
fn mix_then_y_new_ids_reach_the_blocks_inside_every_split_path() {
    let mut chain = mix_then_y_chain();
    let before = all_ids(&chain.blocks);
    assign_new_block_ids(&mut chain);
    let after = all_ids(&chain.blocks);
    assert_eq!(after.len(), before.len());
    let kept: Vec<&String> = after.iter().filter(|id| before.contains(id)).collect();
    assert!(kept.is_empty(), "ids kept from the source chain: {kept:?}");
}
