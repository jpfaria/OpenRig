//! #328 — grouping a return's tail routes by the split paths they run.

use domain::ids::BlockId;
use project::block::PathRef;

use super::{group_routes_by_paths, SegmentPaths};

fn only(paths: &[usize]) -> SegmentPaths {
    SegmentPaths::Only(
        paths
            .iter()
            .map(|&path| PathRef {
                split: BlockId("y".into()),
                path,
            })
            .collect(),
    )
}

#[test]
fn routes_group_by_path_set_in_first_seen_order() {
    let paths = [only(&[1]), only(&[0]), only(&[1]), only(&[0, 1])];
    assert_eq!(
        group_routes_by_paths(&[0, 1, 2, 3], &paths),
        vec![
            (only(&[1]), vec![0, 2]),
            (only(&[0]), vec![1]),
            (only(&[0, 1]), vec![3]),
        ]
    );
}

#[test]
fn a_three_path_y_groups_each_leaf_set_on_its_own() {
    let paths = [only(&[0, 2]), only(&[1]), only(&[0, 2])];
    assert_eq!(
        group_routes_by_paths(&[0, 1, 2], &paths),
        vec![(only(&[0, 2]), vec![0, 2]), (only(&[1]), vec![1])],
        "#328 §11.3: routes hearing the same leaves share one return pipeline"
    );
}

#[test]
fn a_split_free_chain_is_one_group_with_every_route() {
    let paths = [SegmentPaths::None, SegmentPaths::None];
    assert_eq!(
        group_routes_by_paths(&[0, 1], &paths),
        vec![(SegmentPaths::None, vec![0, 1])],
        "#328: a chain with no Y split keeps its one return pipeline, byte-identical"
    );
}

#[test]
fn no_routes_is_one_empty_group() {
    assert_eq!(
        group_routes_by_paths(&[], &[]),
        vec![(SegmentPaths::None, vec![])],
        "the return pipeline with no tail still exists, as before #328"
    );
}
