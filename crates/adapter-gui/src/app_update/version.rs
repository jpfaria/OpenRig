//! Responsibility: decides whether a release tag is newer than the running version.

/// `major.minor.patch` plus whether a `-prerelease` suffix is present.
fn parse(version: &str) -> Option<([u64; 3], bool)> {
    let version = version.trim().trim_start_matches('v');
    let (core, pre) = match version.split_once('-') {
        Some((core, _)) => (core, true),
        None => (version, false),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let triple = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some((triple, pre))
}

/// A final release outranks a prerelease of the same `major.minor.patch`.
/// Anything unparsable is never offered.
pub(super) fn is_newer(latest_tag: &str, current: &str) -> bool {
    let (Some((latest, latest_pre)), Some((running, running_pre))) =
        (parse(latest_tag), parse(current))
    else {
        return false;
    };
    latest > running || (latest == running && running_pre && !latest_pre)
}

#[cfg(test)]
#[path = "version_tests.rs"]
mod tests;
