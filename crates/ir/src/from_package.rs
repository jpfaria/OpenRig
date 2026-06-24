//! Generic IR instantiation from a `plugin_loader::LoadedPackage`.
//!
//! Resolves the user's parameter set against the manifest's capture
//! grid, loads the WAV, and builds a mono/stereo IR processor matching
//! the requested layout. Mono IRs in stereo chains are wrapped as a
//! dual-mono pair.
//!
//! Issue: #287

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{
    wrap_with_output_gain_db, AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor,
};
use plugin_loader::manifest::Backend;
use plugin_loader::LoadedPackage;

use crate::{build_mono_ir_processor_from_wav, build_stereo_ir_processor_from_wav, IrAsset};

/// Build a [`BlockProcessor`] from a disk-backed IR package.
///
/// This is the cab/body path: 100% wet convolution with the capture's
/// audit-baseline output gain applied. For a reverb wet path that blends
/// dry/wet itself, use [`build_convolution_from_package`] and apply the
/// dry/wet mixer on top.
pub fn build_from_package(
    package: &LoadedPackage,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (processor, capture_audit_db) =
        build_convolution_from_package(package, params, sample_rate, layout)?;
    // Issue #491 + #514 + #655: post-convolution output gain (dB). A pure
    // IR is gain-passive, but the perceived level varies a lot (cab/body
    // shapers attenuate lows differently), so each capture ships an audit
    // baseline (`capture.output_gain_db`, manifest-level as fallback).
    // Since #655 the user-facing `output_db` knob is the absolute applied
    // level and overrides the baseline when present; legacy presets without
    // the param keep their audit loudness (volume invariant #10).
    let user_output_db = params.get_f32("output_db");
    let applied_db = resolve_output_db(
        user_output_db,
        capture_audit_db,
        package.manifest.output_gain_db,
    );
    Ok(wrap_with_output_gain_db(processor, applied_db))
}

/// Build the raw, gain-passive convolution [`BlockProcessor`] for the IR
/// capture matched by `params`, returning it alongside the matched capture's
/// audit-baseline `output_gain_db` (so the cab path can wrap it).
///
/// Resolves the capture grid, loads the WAV, and matches the file's channel
/// count to the requested layout (true-stereo for 2-channel, dual-mono wrap
/// for a mono IR in a stereo chain). No output gain is applied — the caller
/// owns the level strategy (cab: audit baseline; reverb #733: dry/wet mix).
pub fn build_convolution_from_package(
    package: &LoadedPackage,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<f32>)> {
    let (parameters, captures) = match &package.manifest.backend {
        Backend::Ir {
            parameters,
            captures,
        } => (parameters, captures),
        _ => bail!(
            "ir::build_convolution_from_package called with non-IR backend (model `{}`)",
            package.manifest.id
        ),
    };
    let capture = plugin_loader::dispatch::resolve_capture(parameters, captures, params)
        .ok_or_else(|| {
            anyhow!(
                "no IR capture matches user params for `{}`",
                package.manifest.id
            )
        })?;
    let path = package.root.join(&capture.file);
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 IR path: {path:?}"))?;
    let asset = IrAsset::load_from_wav(path_str)?;
    let channels = asset.channel_count();
    drop(asset);
    let processor = match (layout, channels) {
        (AudioChannelLayout::Mono, 1) => {
            BlockProcessor::Mono(build_mono_ir_processor_from_wav(path_str, sample_rate)?)
        }
        (AudioChannelLayout::Stereo, 2) => {
            BlockProcessor::Stereo(build_stereo_ir_processor_from_wav(path_str, sample_rate)?)
        }
        (AudioChannelLayout::Stereo, 1) => {
            let left = build_mono_ir_processor_from_wav(path_str, sample_rate)?;
            let right = build_mono_ir_processor_from_wav(path_str, sample_rate)?;
            BlockProcessor::Stereo(Box::new(DualMonoIr { left, right }))
        }
        (AudioChannelLayout::Mono, n) => bail!(
            "IR `{}` has {n} channels but block layout is Mono",
            package.manifest.id
        ),
        (_, n) => bail!(
            "IR `{}` has unsupported channel count {n}",
            package.manifest.id
        ),
    };
    Ok((processor, capture.output_gain_db))
}

/// Choose the audit-baseline dB to apply, preferring the per-capture
/// value over the manifest-level fallback. Issue #514.
pub(crate) fn select_audit_db(capture_db: Option<f32>, manifest_db: Option<f32>) -> Option<f32> {
    capture_db.or(manifest_db)
}

/// Resolve the output gain (dB) the IR processor applies. Mirrors NAM's
/// absolute-knob model (issue #655): the user `output_db` param IS the
/// applied output level and wins when present (even `0.0`); when absent —
/// legacy presets saved before the knob existed — it falls back to the
/// audit baseline (per-capture, then manifest) so loudness is unchanged
/// (volume invariant #10).
pub(crate) fn resolve_output_db(
    user_param_db: Option<f32>,
    capture_audit_db: Option<f32>,
    manifest_audit_db: Option<f32>,
) -> Option<f32> {
    user_param_db.or_else(|| select_audit_db(capture_audit_db, manifest_audit_db))
}

#[cfg(test)]
#[path = "from_package_tests.rs"]
mod tests;

/// Register this crate's builder in the global package-builders table.
pub fn register_builder() {
    plugin_loader::package_builders::register(
        plugin_loader::package_builders::BackendKind::Ir,
        build_from_package,
    );
}

struct DualMonoIr {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

impl StereoProcessor for DualMonoIr {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}
