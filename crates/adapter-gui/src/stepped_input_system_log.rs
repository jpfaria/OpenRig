//! Responsibility: saves the system's audio log next to a stepped-input mark.
//!
//! #1081: a mark showed what OpenRig's input looked like, never what the
//! system did before it broke, and macOS drops that log within a day. Each
//! mark now keeps the last [`LOG_WINDOW`] of the audio log in [`LOG_FILE`]:
//! the audio daemon and its clients (OpenRig included) — which processes
//! started or stopped IO on the device, the HAL's errors, format changes.
//! The window reaches back past the trip, so it covers the moment the input
//! broke even when nothing was played for a while.

use std::fs;
use std::io;
use std::path::Path;

/// The file the log is kept in, inside the mark folder.
pub(crate) const LOG_FILE: &str = "system-audio-log.txt";
/// How far back the log reaches from the mark.
const LOG_WINDOW: &str = "300s";
/// The audio daemon, the device drivers and every process's audio client.
const PREDICATE: &str = "process == \"coreaudiod\" OR process CONTAINS[c] \"audio\" \
     OR subsystem BEGINSWITH \"com.apple.coreaudio\" OR subsystem BEGINSWITH \"com.apple.audio\"";

/// Write what `read_log` returns into `dir`. Nothing is written when the log
/// cannot be read.
pub(crate) fn write_system_log(
    dir: &Path,
    read_log: &dyn Fn() -> io::Result<String>,
) -> io::Result<()> {
    let text = read_log()?;
    fs::write(dir.join(LOG_FILE), text)
}

/// The arguments of the `log show` call.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn audio_log_args() -> Vec<String> {
    [
        "show",
        "--last",
        LOG_WINDOW,
        "--style",
        "compact",
        "--predicate",
        PREDICATE,
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

/// The system's audio log of the last [`LOG_WINDOW`].
#[cfg(target_os = "macos")]
pub(crate) fn read_audio_log() -> io::Result<String> {
    let output = std::process::Command::new("/usr/bin/log")
        .args(audio_log_args())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Only macOS has the log this reads.
#[cfg(not(target_os = "macos"))]
pub(crate) fn read_audio_log() -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no system audio log on this platform",
    ))
}

#[cfg(test)]
#[path = "stepped_input_system_log_tests.rs"]
mod tests;
