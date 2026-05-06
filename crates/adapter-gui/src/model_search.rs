//! Search and filter logic for block model lists.
//!
//! Substring match ignoring case, hyphens, and spaces. Searches in
//! `brand + display_name` (concatenated after normalization) so users
//! can type either part — or a slice spanning both — and find the model.

use crate::BlockModelPickerItem;

pub(crate) fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub(crate) fn model_matches(query: &str, name: &str, brand: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let needle = normalize(query);
    let haystack = format!("{}{}", normalize(brand), normalize(name));
    haystack.contains(&needle)
}

pub(crate) fn filter_models(
    items: &[BlockModelPickerItem],
    query: &str,
) -> Vec<BlockModelPickerItem> {
    if query.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .filter(|item| model_matches(query, item.display_name.as_str(), item.brand.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "model_search_tests.rs"]
mod tests;
