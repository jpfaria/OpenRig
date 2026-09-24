//! Responsibility: finds the only audio processor class a VST3 module exposes.

use crate::host::Vst3PluginClass;

/// The uid of the module's audio processor when it exposes exactly one
/// "Audio Module Class", else `None`.
pub(crate) fn sole_audio_module_uid(classes: &[Vst3PluginClass]) -> Option<[u8; 16]> {
    let mut processors = classes
        .iter()
        .filter(|c| c.category.contains("Audio Module Class"));
    match (processors.next(), processors.next()) {
        (Some(only), None) => Some(only.uid),
        _ => None,
    }
}

#[cfg(test)]
#[path = "sole_audio_module_tests.rs"]
mod tests;
