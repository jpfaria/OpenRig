use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::PitchBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct PitchModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: PitchBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static PitchModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported pitch model '{}'", model))
}

fn noop_validate(_: &ParameterSet) -> Result<()> {
    Ok(())
}

/// Push every native model into the unified plugin-loader registry.
/// Mirrors block-reverb's pattern (issue #287).
pub fn register_natives() {
    use plugin_loader::manifest::BlockType;
    use plugin_loader::native_runtimes::NativeRuntime;
    use plugin_loader::registry::register_native_simple;

    for definition in MODEL_DEFINITIONS {
        if !matches!(definition.backend_kind, PitchBackendKind::Native) {
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
            BlockType::Pitch,
            runtime,
        );
    }
}

/// Returns true if the model has a usable backend on the current platform.
/// LV2 wrappers report `false` when their plugin binary is missing from
/// `libs/lv2/<platform>/`. Native/NAM/IR/VST3 models report `true` (they are
/// always considered available; per-asset checks happen at instantiation).
pub fn is_model_available(model: &str) -> bool {
    // Issue #606: a non-native id is only available when its disk package is
    // actually in the catalog — see `plugin_loader::registry::model_available`.
    plugin_loader::registry::model_available(
        model,
        |m| MODEL_DEFINITIONS.iter().any(|d| d.id == m),
        |m| AVAILABLE_MODEL_IDS.contains(&m),
    )
}
