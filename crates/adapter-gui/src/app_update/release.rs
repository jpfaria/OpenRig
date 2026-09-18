//! Responsibility: extracts the latest release tag from the GitHub API response.

pub(super) fn latest_release_tag(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = value.get("tag_name")?.as_str()?.trim();
    (!tag.is_empty()).then(|| tag.to_string())
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
