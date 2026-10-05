//! Responsibility: names the routing nodes a parallel stage inserts
//!
//! The chain builder emits them and the anchor placer finds them again by
//! the same name, so the format lives once, here. Both are keyed by the
//! split's own id, so they stay stable at any depth (#328).

/// Prefix of a split node id.
pub const SPLIT_NODE_PREFIX: &str = "__split_";
/// Prefix of a merge node id.
pub const MERGE_NODE_PREFIX: &str = "__merge_";

/// Id of the split node the parallel stage of split `split_id` starts with.
pub fn split_node_id(split_id: &str) -> String {
    format!("{SPLIT_NODE_PREFIX}{split_id}")
}

/// Id of the merge node the merging parallel stage of split `split_id` ends with.
pub fn merge_node_id(split_id: &str) -> String {
    format!("{MERGE_NODE_PREFIX}{split_id}")
}
