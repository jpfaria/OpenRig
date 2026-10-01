//! #328 (spec §5.1) — a click opens what the node stands for.

use super::*;
use crate::chain_graph_fixtures_tests::{
    mix_chain, mix_then_y_chain, FIRST_MIXER_NODE_ID, FIRST_SPLIT_NODE_ID,
};
use crate::chain_graph_ids::{INPUT_NODE_ID, OUTPUT_NODE_ID};
use domain::ids::BlockId;
use project::block::PathRef;

#[test]
fn a_top_level_card_opens_through_its_strip_row() {
    let c = mix_chain();
    assert_eq!(click_action(&c, "pre"), Some(ClickAction::SelectRow(0)));
    assert_eq!(click_action(&c, "post"), Some(ClickAction::SelectRow(2)));
}

#[test]
fn a_path_card_opens_through_its_path() {
    let path = PathRef {
        split: BlockId("sp".into()),
        path: 0,
    };
    assert_eq!(
        click_action(&mix_chain(), "a2"),
        Some(ClickAction::OpenPathBlock { path, index: 1 })
    );
}

#[test]
fn routing_nodes_open_their_editors() {
    let c = mix_chain();
    let sp = || BlockId("sp".into());
    assert_eq!(
        click_action(&c, FIRST_SPLIT_NODE_ID),
        Some(ClickAction::OpenSplitEditor { split: sp() })
    );
    assert_eq!(
        click_action(&c, FIRST_MIXER_NODE_ID),
        Some(ClickAction::OpenMixerEditor { split: sp() })
    );
    assert_eq!(
        click_action(&c, INPUT_NODE_ID),
        Some(ClickAction::OpenChecklist)
    );
    assert_eq!(
        click_action(&c, OUTPUT_NODE_ID),
        Some(ClickAction::OpenChecklist)
    );
    assert_eq!(click_action(&c, "gone"), None);
}

#[test]
fn mix_then_y_the_second_split_node_opens_the_split_editor() {
    let c = mix_then_y_chain();
    assert_eq!(
        click_action(&c, "__split_y"),
        Some(ClickAction::OpenSplitEditor {
            split: BlockId("y".into())
        })
    );
    assert_eq!(
        click_action(&c, "__merge_mx"),
        Some(ClickAction::OpenMixerEditor {
            split: BlockId("mx".into())
        })
    );
    assert_eq!(click_action(&c, "__merge_y"), None, "a Y has no mixer");
}
