//! #328 (spec §5.2) — every graph node id resolves to what it stands for.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide};
use project::endpoint_disables::EndpointNode;

fn block(id: &str, path: Option<PathSide>, index: usize) -> NodeRef {
    NodeRef::Block {
        id: BlockId(id.into()),
        path: path.map(|side| PathRef {
            split: BlockId("sp".into()),
            side,
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
    assert_eq!(
        resolve_node(&c, PATH_A_OUTPUT_NODE_ID),
        Some(NodeRef::Endpoints(EndpointNode::PathAOutput))
    );
    assert_eq!(
        resolve_node(&c, PATH_B_OUTPUT_NODE_ID),
        Some(NodeRef::Endpoints(EndpointNode::PathBOutput))
    );
}

#[test]
fn a_block_node_is_its_block_wherever_it_sits() {
    let c = mix_chain();
    assert_eq!(resolve_node(&c, "pre"), Some(block("pre", None, 0)));
    assert_eq!(resolve_node(&c, "post"), Some(block("post", None, 2)));
    assert_eq!(
        resolve_node(&c, "a2"),
        Some(block("a2", Some(PathSide::A), 1))
    );
    assert_eq!(
        resolve_node(&c, "b1"),
        Some(block("b1", Some(PathSide::B), 0))
    );
}

#[test]
fn split_and_mixer_nodes_name_the_chains_split() {
    let c = mix_chain();
    assert_eq!(resolve_node(&c, SPLIT_NODE_ID), Some(NodeRef::Split));
    assert_eq!(resolve_node(&c, MIXER_NODE_ID), Some(NodeRef::Mixer));
    assert_eq!(resolve_node(&chain(vec![core("x")]), SPLIT_NODE_ID), None);
}

#[test]
fn an_unknown_id_is_nothing() {
    assert_eq!(resolve_node(&mix_chain(), "gone"), None);
}
