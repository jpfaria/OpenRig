//! Responsibility: routes the dynamics crate's public surface.
//! Dynamics implementations.
pub mod model_visual;
mod registry;

use block_core::ModelVisualData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DynBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn dyn_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            DynBackendKind::Native => "NATIVE",
            DynBackendKind::Nam => "NAM",
            DynBackendKind::Ir => "IR",
            DynBackendKind::Lv2 => "LV2",
            DynBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
        thumbnail_path: None,
        available: registry::is_model_available(model_id),
    })
}

pub fn dyn_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn dyn_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn dyn_type_label(model: &str) -> &'static str {
    dyn_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn compressor_supported_models() -> &'static [&'static str] {
    registry::COMPRESSOR_SUPPORTED_MODELS
}

pub fn gate_supported_models() -> &'static [&'static str] {
    registry::GATE_SUPPORTED_MODELS
}

mod compressor_family;
mod dynamics_family;
mod gate_family;

pub use crate::compressor_family::{
    build_compressor_processor, build_compressor_processor_for_layout, compressor_model_schema,
};
pub use crate::dynamics_family::{
    build_dynamics_processor, build_dynamics_processor_for_layout, dynamics_model_schema,
};
pub use crate::gate_family::{
    build_gate_processor, build_gate_processor_for_layout, gate_model_schema,
};

/// Push every native model into the unified plugin-loader registry.
/// Called by `adapter-gui` at startup before plugin discovery freezes
/// the catalog.
pub fn register_natives() {
    registry::register_natives();
}

pub fn is_dyn_model_available(model: &str) -> bool {
    registry::is_model_available(model)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
