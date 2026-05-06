//! Per-model visual color overrides for native filters owned by this crate.
//! Phase 4b of issue #194.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "eq_three_band_basic" => Some(ModelColorOverride {
            panel_bg: Some([0x24, 0x2c, 0x34]),
            panel_text: Some([0x88, 0xa0, 0xc0]),
            brand_strip_bg: Some([0x16, 0x1c, 0x22]),
            model_font: Some("Orbitron"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_visual_tests.rs"]
mod tests;
