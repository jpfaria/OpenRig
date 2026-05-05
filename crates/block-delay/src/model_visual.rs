//! Per-model visual color overrides for native delays owned by this crate.
//! Phase 4b of issue #194 — see `block-amp::model_visual` for rationale.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "analog_warm" => Some(ModelColorOverride {
            panel_bg: Some([0x3a, 0x2a, 0x1a]),
            panel_text: Some([0xd0, 0xb0, 0x80]),
            brand_strip_bg: Some([0x20, 0x18, 0x10]),
            model_font: Some("Dancing Script"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "digital_clean" => Some(ModelColorOverride {
            panel_bg: Some([0x1a, 0x28, 0x3a]),
            panel_text: Some([0x80, 0xb0, 0xe0]),
            brand_strip_bg: Some([0x10, 0x18, 0x24]),
            model_font: Some("Orbitron"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "modulated_delay" => Some(ModelColorOverride {
            panel_bg: Some([0x2a, 0x1a, 0x3a]),
            panel_text: Some([0xb0, 0x90, 0xd0]),
            brand_strip_bg: Some([0x18, 0x10, 0x24]),
            model_font: Some("Dancing Script"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "reverse" => Some(ModelColorOverride {
            panel_bg: Some([0x1a, 0x1a, 0x30]),
            panel_text: Some([0x90, 0x90, 0xd0]),
            brand_strip_bg: Some([0x10, 0x10, 0x20]),
            model_font: Some("Orbitron"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "slapback" => Some(ModelColorOverride {
            panel_bg: Some([0x30, 0x2a, 0x20]),
            panel_text: Some([0xc0, 0xa8, 0x80]),
            brand_strip_bg: Some([0x1e, 0x18, 0x12]),
            model_font: Some("Permanent Marker"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "tape_vintage" => Some(ModelColorOverride {
            panel_bg: Some([0x38, 0x28, 0x18]),
            panel_text: Some([0xd0, 0xb8, 0x90]),
            brand_strip_bg: Some([0x22, 0x18, 0x0e]),
            model_font: Some("Dancing Script"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_visual_tests.rs"]
mod tests;
