//! Responsibility: says where a block can be inserted in a chain row.

#[cfg(test)]
pub fn insertion_slot_indices(block_count: usize) -> Vec<usize> {
    (0..=block_count).collect()
}
