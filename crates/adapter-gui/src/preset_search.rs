//! Search/filter logic for the chain preset bank dropdown (`PresetSelect`).
//!
//! The bank dropdown lists the rig input's saved preset slots. Selecting a
//! slot dispatches `Command::ApplyRigNav { Preset(slot) }`, so a filtered
//! row MUST carry its ORIGINAL slot — the position in the unfiltered bank —
//! not its position in the filtered view. This mirrors the block model
//! picker resolving a click by `model_id` (stable identity) rather than by
//! the filtered row index.
//!
//! The matching predicate (`preset_label_matches`) is the single source of
//! truth shared with `chain_preset_wiring::filter_preset_names` (the load
//! overlay's search), so both search fields behave identically.

/// Case-insensitive substring match. An empty (trimmed) query matches
/// everything. `query_lower` MUST already be trimmed + lowercased by the
/// caller so it is computed once per filter pass, not once per label.
pub(crate) fn preset_label_matches(label: &str, query_lower: &str) -> bool {
    query_lower.is_empty() || label.to_lowercase().contains(query_lower)
}

/// Filter `labels` by `query`, preserving each kept label's ORIGINAL index
/// (its bank slot). Returns `(slot, label)` pairs in original order. An
/// empty query returns every label paired with its index.
pub(crate) fn filter_preset_labels_indexed(labels: &[String], query: &str) -> Vec<(usize, String)> {
    let query_lower = query.trim().to_lowercase();
    labels
        .iter()
        .enumerate()
        .filter(|(_, label)| preset_label_matches(label, &query_lower))
        .map(|(slot, label)| (slot, label.clone()))
        .collect()
}

#[cfg(test)]
#[path = "preset_search_tests.rs"]
mod tests;
