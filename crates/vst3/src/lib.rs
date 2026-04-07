//! VST3 plugin host for OpenRig.
//!
//! Provides a safe Rust wrapper around the VST3 COM API:
//! - Loads `.vst3` bundles via `libloading`
//! - Enumerates plugin classes via `IPluginFactory`
//! - Instantiates `IComponent` + `IAudioProcessor` + `IEditController`
//! - Processes audio in mono or stereo
//! - Reads and writes plugin parameters (normalized 0.0..=1.0)
//! - Scans standard system paths for installed VST3 plugins
//!
//! The API mirrors the `lv2` crate so block crates can use either backend
//! interchangeably.

mod host;
mod processor;
mod stereo;
pub mod discovery;

pub use host::{Vst3Plugin, Vst3ParamInfo, Vst3PluginClass};
pub use processor::Vst3Processor;
pub use stereo::StereoVst3Processor;
pub use discovery::{Vst3PluginInfo, scan_vst3_bundle, scan_system_vst3, system_vst3_paths};

use anyhow::Result;
use std::path::Path;
use block_core::{AudioChannelLayout, BlockProcessor};

/// Build a ready-to-use VST3 `BlockProcessor`.
///
/// - `bundle_path`: path to the `.vst3` bundle directory
/// - `plugin_uid`:  16-byte class ID (from `IPluginFactory::getClassInfo`)
/// - `sample_rate`: audio sample rate in Hz
/// - `layout`:      `Mono` or `Stereo`
/// - `params`:      `(param_id, normalized_value)` pairs to set initially
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use block_core::AudioChannelLayout;
/// use vst3_host::build_vst3_processor;
///
/// let uid = [0u8; 16]; // fill with real UID from discovery
/// let processor = build_vst3_processor(
///     Path::new("/Library/Audio/Plug-Ins/VST3/MyPlugin.vst3"),
///     &uid,
///     44100.0,
///     AudioChannelLayout::Stereo,
///     &[(0, 0.5)],
/// ).unwrap();
/// ```
pub fn build_vst3_processor(
    bundle_path: &Path,
    plugin_uid: &[u8; 16],
    sample_rate: f64,
    layout: AudioChannelLayout,
    params: &[(u32, f64)],
) -> Result<BlockProcessor> {
    const BLOCK_SIZE: usize = 512;

    match layout {
        AudioChannelLayout::Mono => {
            let plugin = Vst3Plugin::load(
                bundle_path,
                plugin_uid,
                sample_rate,
                2, // load as stereo even for mono (most VST3 plugins are stereo-only)
                BLOCK_SIZE,
                params,
            )?;
            Ok(BlockProcessor::Mono(Box::new(Vst3Processor::new(plugin))))
        }
        AudioChannelLayout::Stereo => {
            let plugin = Vst3Plugin::load(
                bundle_path,
                plugin_uid,
                sample_rate,
                2,
                BLOCK_SIZE,
                params,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(StereoVst3Processor::new(plugin))))
        }
    }
}
