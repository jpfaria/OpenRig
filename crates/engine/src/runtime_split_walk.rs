//! Responsibility: visits every runtime node including those inside split paths.

use crate::runtime_state::{BlockRuntimeNode, RuntimeProcessor};

/// Call `visit` on every node of `nodes`, then on the nodes of both paths of
/// every split among them (#328). Select options are not visited — the
/// walkers that use this treated them that way before splits existed.
pub(crate) fn for_each_node<'a, F>(nodes: &'a [BlockRuntimeNode], visit: &mut F)
where
    F: FnMut(&'a BlockRuntimeNode),
{
    for node in nodes {
        visit(node);
        if let RuntimeProcessor::Split(split) = &node.processor {
            for_each_node(&split.a, visit);
            for_each_node(&split.b, visit);
        }
    }
}

#[cfg(test)]
#[path = "runtime_split_walk_tests.rs"]
mod tests;
