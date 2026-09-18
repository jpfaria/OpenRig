//! Responsibility: picks the release the running app is offered as an update.

use super::release::latest_release_tag;
use super::version::is_newer;

/// The newer release's version without the `v`, or `None` when the fetch
/// failed or the running build is already current.
pub(super) fn offered_update(release_json: Option<&str>, current: &str) -> Option<String> {
    let tag = latest_release_tag(release_json?)?;
    is_newer(&tag, current).then(|| tag.trim_start_matches('v').to_string())
}

#[cfg(test)]
#[path = "offer_tests.rs"]
mod tests;
