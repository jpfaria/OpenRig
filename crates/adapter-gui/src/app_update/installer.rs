//! Responsibility: opens Terminal running the macOS installer command.

pub(super) const INSTALL_COMMAND: &str =
    "curl -fsSL https://raw.githubusercontent.com/jpfaria/OpenRig/develop/scripts/install-macos.sh | bash";

/// `osascript` arguments that run [`INSTALL_COMMAND`] in a new Terminal
/// window and bring Terminal to the front so the user sees the progress.
pub(super) fn osascript_args() -> Vec<String> {
    [
        format!("tell application \"Terminal\" to do script \"{INSTALL_COMMAND}\""),
        "tell application \"Terminal\" to activate".to_string(),
    ]
    .into_iter()
    .flat_map(|line| ["-e".to_string(), line])
    .collect()
}

pub(super) fn launch_installer() {
    if let Err(e) = std::process::Command::new("osascript")
        .args(osascript_args())
        .spawn()
    {
        log::warn!("could not open Terminal for the update: {e}");
    }
}

#[cfg(test)]
#[path = "installer_tests.rs"]
mod tests;
