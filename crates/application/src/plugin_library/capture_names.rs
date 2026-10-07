//! Responsibility: gives each capture of a package its original name.
//!
//! A TONE3000 install records the name each capture had on TONE3000; any
//! other capture is known by its file name.

use std::path::Path;

use plugin_loader::manifest::GridCapture;

use crate::tone3000::source_stamp::read_capture_names;

/// The original name of every capture in `captures`, in the same order.
pub fn capture_names(package_root: &Path, captures: &[GridCapture]) -> Vec<String> {
    let recorded = read_capture_names(package_root);
    captures
        .iter()
        .map(|capture| {
            recorded.get(&capture.file).cloned().unwrap_or_else(|| {
                capture
                    .file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
        })
        .collect()
}
