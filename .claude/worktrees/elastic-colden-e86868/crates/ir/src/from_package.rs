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
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor};
use plugin_loader::manifest::Backend;
use plugin_loader::LoadedPackage;

use crate::{build_mono_ir_processor_from_wav, build_stereo_ir_processor_from_wav, IrAsset};

/// Build a [`BlockProcessor`] from a disk-backed IR package.
pub fn build_from_package(
    package: &LoadedPackage,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (parameters, captures) = match &package.manifest.backend {
        Backend::Ir {
            parameters,
            captures,
        } => (parameters, captures),
        _ => bail!(
            "ir::build_from_package called with non-IR backend (model `{}`)",
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
    match (layout, channels) {
        (AudioChannelLayout::Mono, 1) => Ok(BlockProcessor::Mono(
            build_mono_ir_processor_from_wav(path_str, sample_rate)?,
        )),
        (AudioChannelLayout::Stereo, 2) => Ok(BlockProcessor::Stereo(
            build_stereo_ir_processor_from_wav(path_str, sample_rate)?,
        )),
        (AudioChannelLayout::Stereo, 1) => {
            let left = build_mono_ir_processor_from_wav(path_str, sample_rate)?;
            let right = build_mono_ir_processor_from_wav(path_str, sample_rate)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoIr { left, right })))
        }
        (AudioChannelLayout::Mono, n) => bail!(
            "IR `{}` has {n} channels but block layout is Mono",
            package.manifest.id
        ),
        (_, n) => bail!(
            "IR `{}` has unsupported channel count {n}",
            package.manifest.id
        ),
    }
}

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
