//! Responsibility: decides the audio mode a disk package runs in.

use block_core::ModelAudioMode;
use plugin_loader::dispatch::{Lv2Port, Lv2PortRole};
use plugin_loader::manifest::Backend;

use super::lv2_bundle_ports::lv2_bundle_ports;

/// Audio mode for a model that lives only in `plugin_loader::registry`.
///
/// Streams are always stereo internally (CLAUDE.md invariant #5), so
/// mono-native plugins (NAM, IR, LV2 1in/1out) run as `DualMono`: one
/// instance per channel.
pub(crate) fn disk_package_audio_mode(package: &plugin_loader::LoadedPackage) -> ModelAudioMode {
    match &package.manifest.backend {
        Backend::Lv2 {
            plugin_uri,
            binaries,
        } => lv2_bundle_ports(package, plugin_uri, binaries)
            .map(|ports| lv2_audio_mode(&ports))
            .unwrap_or(ModelAudioMode::DualMono),
        _ => ModelAudioMode::DualMono,
    }
}

/// Audio mode an LV2 plugin's audio ports call for. Must agree with the
/// builder `lv2::build_from_package` picks for the same shape
/// (`docs/development/file-organization.md`, issue #130): a 2in/2out plugin
/// forced to `DualMono` ran averaged per channel and printed L == R (#938).
/// A 2in/1out plugin is a sidechain — mono as far as the chain is concerned.
pub(crate) fn lv2_audio_mode(ports: &[Lv2Port]) -> ModelAudioMode {
    let count = |role: Lv2PortRole| ports.iter().filter(|p| p.role == role).count();
    match (count(Lv2PortRole::AudioIn), count(Lv2PortRole::AudioOut)) {
        (1, 2) => ModelAudioMode::MonoToStereo,
        (2, 2) => ModelAudioMode::TrueStereo,
        _ => ModelAudioMode::DualMono,
    }
}

#[cfg(test)]
#[path = "disk_audio_mode_tests.rs"]
mod tests;
