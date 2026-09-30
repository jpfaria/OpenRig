//! #328 — grouping a return's tail routes by the split paths they run.

use super::{group_routes_by_paths, SegmentPaths};

#[test]
fn routes_group_by_path_set_in_first_seen_order() {
    let paths = [
        SegmentPaths::B,
        SegmentPaths::A,
        SegmentPaths::B,
        SegmentPaths::AB,
    ];
    assert_eq!(
        group_routes_by_paths(&[0, 1, 2, 3], &paths),
        vec![
            (SegmentPaths::B, vec![0, 2]),
            (SegmentPaths::A, vec![1]),
            (SegmentPaths::AB, vec![3]),
        ]
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
