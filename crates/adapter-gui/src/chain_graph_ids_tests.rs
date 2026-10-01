//! #328 (spec §5.2) — every graph node id resolves to what it stands for.

use super::*;
use crate::chain_graph_fixtures_tests::{
    chain, core, mix_chain, mix_then_y_chain, y_chain, FIRST_MIXER_NODE_ID, FIRST_SPLIT_NODE_ID,
};
use domain::ids::BlockId;
use project::block::PathRef;
use project::endpoint_disables::EndpointNode;

fn block(id: &str, path: Option<usize>, index: usize) -> NodeRef {
    NodeRef::Block {
        id: BlockId(id.into()),
        path: path.map(|path| PathRef {
            split: BlockId("sp".into()),
            path,
        }),
        index,
    }
}

#[test]
fn io_nodes_name_their_endpoint_set() {
    let c = mix_chain();
    assert_eq!(
        resolve_node(&c, INPUT_NODE_ID),
        Some(NodeRef::Endpoints(EndpointNode::Input))
    );
    assert_eq!(
        resolve_node(&c, OUTPUT_NODE_ID),
        Some(NodeRef::Endpoints(EndpointNode::Output))
    );
}

#[test]
fn every_y_leaf_has_its_own_output_node() {
    let c = y_chain();
    for path in 0..2 {
        let leaf = PathRef {
            split: BlockId("sp".into()),
            path,
        };
        assert_eq!(
            resolve_node(&c, &leaf_output_node_id(&leaf)),
            Some(NodeRef::Endpoints(EndpointNode::PathOutput(leaf)))
        );
    }
    assert_eq!(
        resolve_node(&mix_chain(), "__out_sp_0"),
        None,
        "a Mix path ends in the mixer, not in an output node"
    );
    assert_eq!(resolve_node(&c, "__out_sp_2"), None, "no path C");
}

#[test]
fn a_block_node_is_its_block_wherever_it_sits() {
    let c = mix_chain();
    assert_eq!(resolve_node(&c, "pre"), Some(block("pre", None, 0)));
    assert_eq!(resolve_node(&c, "post"), Some(block("post", None, 2)));
    assert_eq!(resolve_node(&c, "a2"), Some(block("a2", Some(0), 1)));
    assert_eq!(resolve_node(&c, "b1"), Some(block("b1", Some(1), 0)));
}

#[test]
fn split_and_mixer_nodes_name_the_chains_split() {
    let c = mix_chain();
    let sp = || BlockId("sp".into());
    assert_eq!(
        resolve_node(&c, FIRST_SPLIT_NODE_ID),
        Some(NodeRef::Split { id: sp() })
    );
    assert_eq!(
        resolve_node(&c, FIRST_MIXER_NODE_ID),
        Some(NodeRef::Mixer { id: sp() })
    );
    assert_eq!(
        resolve_node(&chain(vec![core("x")]), FIRST_SPLIT_NODE_ID),
        None
    );
}

#[test]
fn an_unknown_id_is_nothing() {
    assert_eq!(resolve_node(&mix_chain(), "gone"), None);
}

#[test]
fn mix_then_y_each_routing_node_names_its_own_split() {
    let c = mix_then_y_chain();
    let id = |s: &str| BlockId(s.into());
    assert_eq!(
        resolve_node(&c, "__split_mx"),
        Some(NodeRef::Split { id: id("mx") })
    );
    assert_eq!(
        resolve_node(&c, "__merge_mx"),
        Some(NodeRef::Mixer { id: id("mx") })
    );
    assert_eq!(
        resolve_node(&c, "__split_y"),
        Some(NodeRef::Split { id: id("y") })
    );
    assert_eq!(resolve_node(&c, "__merge_y"), None, "a Y has no mixer");
    assert_eq!(resolve_node(&c, "__split_gone"), None);
}

#[test]
fn mix_then_y_a_card_in_the_ys_lane_names_the_ys_path() {
    assert_eq!(
        resolve_node(&mix_then_y_chain(), "yb"),
        Some(NodeRef::Block {
            id: BlockId("yb".into()),
            path: Some(PathRef {
                split: BlockId("y".into()),
                path: 1,
            }),
            index: 0,
        })
    );
}
