//! Per-model visual color overrides for native dynamics owned by this crate.
//! Phase 4b of issue #194.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "compressor_studio_clean" => Some(ModelColorOverride {
            panel_bg: Some([0x28, 0x30, 0x2a]),
            panel_text: Some([0x90, 0xb0, 0x90]),
            brand_strip_bg: Some([0x18, 0x20, 0x1a]),
            model_font: Some("Orbitron"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "gate_basic" => Some(ModelColorOverride {
            panel_bg: Some([0x30, 0x28, 0x28]),
            panel_text: Some([0xb0, 0x90, 0x90]),
            brand_strip_bg: Some([0x20, 0x18, 0x18]),
            model_font: Some("Permanent Marker"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_visual_tests.rs"]
mod tests;
