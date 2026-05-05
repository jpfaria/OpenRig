use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor};

use crate::shared::build_dual_mono_from_builder;
use crate::DelayBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct DelayModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: DelayBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
}

pub(crate) fn build_dual_mono_delay_processor<F>(
    layout: AudioChannelLayout,
    builder: F,
) -> Result<BlockProcessor>
where
    F: Fn() -> Result<Box<dyn MonoProcessor>>,
{
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(builder()?)),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(build_dual_mono_from_builder(
            builder,
        )?)),
    }
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static DelayModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported delay model '{}'", model))
}

fn noop_validate(_: &block_core::param::ParameterSet) -> anyhow::Result<()> {
    Ok(())
}

/// Push every native model into the unified plugin-loader registry.
/// Disk-backed models (NAM/IR/LV2/VST3) stay in the legacy per-block path
/// until the disk-backend dispatchers move into plugin-loader too.
///
/// Issue: #287
pub fn register_natives() {
    use plugin_loader::manifest::BlockType;
    use plugin_loader::native_runtimes::NativeRuntime;
    use plugin_loader::registry::register_native_simple;

    for definition in MODEL_DEFINITIONS {
        if !matches!(definition.backend_kind, DelayBackendKind::Native) {
            continue;
        }
        let runtime = NativeRuntime {
            schema: definition.schema,
            validate: noop_validate,
            build: definition.build,
        };
        let brand = if definition.brand.is_empty() {
            Some("openrig")
        } else {
            Some(definition.brand)
        };
        register_native_simple(
            definition.id,
            definition.display_name,
            brand,
            BlockType::Delay,
            runtime,
        );
    }
}
