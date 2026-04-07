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
pub mod param_channel;
pub mod param_registry;
pub mod component_handler;
pub mod discovery;
pub mod catalog;
pub mod editor;

pub use host::{Vst3Plugin, Vst3ParamInfo, Vst3PluginClass};
pub use editor::{Vst3EditorHandle, open_vst3_editor_window};
pub use processor::Vst3Processor;
pub use stereo::StereoVst3Processor;
pub use discovery::{Vst3PluginInfo, scan_vst3_bundle, scan_vst3_bundle_light, scan_system_vst3, system_vst3_paths, resolve_vst3_bundle};
pub use catalog::{
    Vst3CatalogEntry, init_vst3_catalog, vst3_catalog, find_vst3_plugin,
    vst3_model_ids, vst3_model_visual, make_model_id, resolve_uid_for_model,
};
pub use param_channel::{Vst3ParamChannel, Vst3ParamUpdate, vst3_param_channel};
pub use param_registry::{register_vst3_channel, lookup_vst3_channel};

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
                2,
                BLOCK_SIZE,
                params,
            )?;
            Ok(BlockProcessor::Mono(Box::new(Vst3Processor::new(plugin, None))))
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
            Ok(BlockProcessor::Stereo(Box::new(StereoVst3Processor::new(plugin, None))))
        }
    }
}

/// Build a VST3 `BlockProcessor` connected to a parameter channel so that
/// knob movements in the native plugin GUI are applied to the audio processor.
///
/// - `param_channel`: the `Vst3ParamChannel` previously registered via
///   `register_vst3_channel`.  The GUI editor will push updates onto the same
///   `Arc`; the processor drains them before each processing block.
pub fn build_vst3_processor_with_channel(
    bundle_path: &Path,
    plugin_uid: &[u8; 16],
    sample_rate: f64,
    layout: AudioChannelLayout,
    params: &[(u32, f64)],
    param_channel: Vst3ParamChannel,
) -> Result<BlockProcessor> {
    const BLOCK_SIZE: usize = 512;

    match layout {
        AudioChannelLayout::Mono => {
            let plugin = Vst3Plugin::load(
                bundle_path,
                plugin_uid,
                sample_rate,
                2,
                BLOCK_SIZE,
                params,
            )?;
            Ok(BlockProcessor::Mono(Box::new(Vst3Processor::new(plugin, Some(param_channel)))))
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
            Ok(BlockProcessor::Stereo(Box::new(StereoVst3Processor::new(plugin, Some(param_channel)))))
        }
    }
}
