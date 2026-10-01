//! #328: the walk reaches the nodes inside every path of a split.

use domain::ids::BlockId;
use project::block::split_params::default_split_params;

use super::for_each_node;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_split::test_support::gain_node;
use crate::runtime_state::RuntimeProcessor;

#[test]
fn the_walk_visits_every_path_of_a_split() {
    let mut split = gain_node("split", 1.0);
    split.processor = RuntimeProcessor::Split(SplitRuntimeState::new(
        true,
        vec![
            vec![gain_node("amp_a", 1.0)],
            vec![gain_node("amp_b", 1.0)],
            vec![gain_node("amp_c", 1.0)],
        ],
        SplitKnobs::from_params(&default_split_params(3), 3),
        &BlockId("split".into()),
    ));
    let nodes = vec![gain_node("pre", 1.0), split, gain_node("post", 1.0)];
    let mut seen = Vec::new();
    for_each_node(&nodes, &mut |node| seen.push(node.block_id.0.clone()));
    assert_eq!(
        seen,
        vec!["pre", "split", "amp_a", "amp_b", "amp_c", "post"]
    );
}
