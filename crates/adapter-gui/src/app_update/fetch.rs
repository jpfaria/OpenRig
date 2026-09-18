//! Responsibility: downloads the latest-release JSON from GitHub.

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/jpfaria/OpenRig/releases/latest";

/// Runs off the GUI thread. `curl` ships with macOS, so no HTTP/TLS stack is
/// linked into the app for one request. Any failure (offline, rate limit)
/// is `None`: the label just stays a label.
pub(super) fn fetch_latest_release_json() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "10",
            "-H",
            "Accept: application/vnd.github+json",
            LATEST_RELEASE_API,
        ])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
