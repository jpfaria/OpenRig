//! Responsibility: decides what a click on a graph node opens.
//!
//! #328 (spec §5.1): a block card opens its block editor (a top-level card
//! through the strip's own row flow, a path card through `open-path-block`),
//! the split and mixer nodes open their knob editors, an input or output node
//! opens the endpoint checklist.

use project::block::PathRef;
use project::chain::Chain;

use crate::chain_graph_ids::{resolve_node, NodeRef};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ClickAction {
    SelectRow(usize),
    OpenPathBlock { path: PathRef, index: usize },
    OpenSplitEditor,
    OpenMixerEditor,
    OpenChecklist,
}

pub(crate) fn click_action(chain: &Chain, node_id: &str) -> Option<ClickAction> {
    Some(match resolve_node(chain, node_id)? {
        NodeRef::Block {
            path: None, index, ..
        } => ClickAction::SelectRow(index),
        NodeRef::Block {
            path: Some(path),
            index,
            ..
        } => ClickAction::OpenPathBlock { path, index },
        NodeRef::Split => ClickAction::OpenSplitEditor,
        NodeRef::Mixer => ClickAction::OpenMixerEditor,
        NodeRef::Endpoints(_) => ClickAction::OpenChecklist,
    })
}

#[cfg(test)]
#[path = "graph_click_tests.rs"]
mod tests;
