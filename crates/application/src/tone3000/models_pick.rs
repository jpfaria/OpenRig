//! Responsibility: picks the distinct downloadable captures of a tone.
//!
//! `GET /models` lists one row per (capture, size) and several rows can
//! point at the same file; rows without a file cannot be installed.

use std::collections::HashSet;

use super::api_types::Model;

/// Rows with a file, first row per file name kept, in API order.
pub fn unique_models(rows: Vec<Model>) -> Vec<Model> {
    let mut seen = HashSet::new();
    rows.into_iter()
        .filter(|row| match row.model_url.as_deref() {
            Some(url) => seen.insert(file_name(url).to_string()),
            None => false,
        })
        .collect()
}

/// Last path segment of a URL, query and fragment stripped.
pub fn file_name(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/').next().unwrap_or(path)
}
