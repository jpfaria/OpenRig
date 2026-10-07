//! Responsibility: asks the user for the capture files of a new plugin.
//!
//! The native dialog lives in its own file, like the backing-track chooser,
//! so the editor wiring stays free of `rfd`.

use std::path::PathBuf;

use application::plugin_library::CaptureBackend;
use rfd::FileDialog;

/// The files the user picked for `backend`; empty when the dialog was cancelled.
pub(crate) fn choose_capture_files(backend: CaptureBackend) -> Vec<PathBuf> {
    let (label, extensions): (&str, &[&str]) = match backend {
        CaptureBackend::Nam => ("NAM", &["nam"]),
        CaptureBackend::Ir => ("IR", &["wav"]),
    };
    FileDialog::new()
        .add_filter(label, extensions)
        .pick_files()
        .unwrap_or_default()
}
