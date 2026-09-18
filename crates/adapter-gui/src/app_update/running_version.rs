//! Responsibility: resolves the version the update check treats as running.

/// `OPENRIG_UPDATE_CURRENT_VERSION` lets a current build pretend to be older,
/// so the update button can be exercised without an actual new release.
pub(super) fn current_version(env_override: Option<String>, compiled: &str) -> String {
    env_override
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| compiled.to_string())
}

#[cfg(test)]
#[path = "running_version_tests.rs"]
mod tests;
