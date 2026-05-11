use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};
use plugin_loader::manifest::BlockType;
use plugin_loader::native_runtimes::NativeRuntime;
use plugin_loader::registry::register_native_simple;

use crate::CabBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct CabModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: CabBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
}
include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static CabModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported cab model '{}'", model))
}

/// Push every native cab model into the unified [`plugin_loader`]
/// registry. Disk-backed cabs (IR, LV2) stay in the legacy per-block
/// path until the disk-backend dispatchers move into plugin-loader too.
///
/// Called once at process startup by `adapter-gui`, before
/// `plugin_loader::registry::init` freezes the catalog.
pub fn register_natives() {
    let brand_native = "openrig";
    for definition in MODEL_DEFINITIONS {
        if !matches!(definition.backend_kind, CabBackendKind::Native) {
            continue;
        }
        let runtime = NativeRuntime {
            schema: definition.schema,
            validate: definition.validate,
            build: definition.build,
        };
        let brand = if definition.brand.is_empty() {
            Some(brand_native)
        } else {
            Some(definition.brand)
        };
        register_native_simple(
            definition.id,
            definition.display_name,
            brand,
            BlockType::Cab,
            runtime,
        );
    }
}
