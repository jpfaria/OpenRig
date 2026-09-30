//! Responsibility: names the routing nodes a parallel stage inserts
//!
//! The chain builder emits them and the anchor placer finds them again by
//! the same name, so the format lives once, here.

/// Id of the split node the `n`-th parallel stage (1-based) starts with.
pub fn split_node_id(n: usize) -> String {
    format!("__split_{n}")
}

/// Id of the merge node the `n`-th merging parallel stage (1-based) ends with.
pub fn merge_node_id(n: usize) -> String {
    format!("__merge_{n}")
}
