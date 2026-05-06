//! Per-model visual color overrides for native wahs owned by this crate.
//! Phase 4b of issue #194.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "cry_classic" => Some(ModelColorOverride {
            panel_bg: Some([0x34, 0x24, 0x1a]),
            panel_text: Some([0xc8, 0xa0, 0x70]),
            brand_strip_bg: Some([0x22, 0x16, 0x0e]),
            model_font: Some("Permanent Marker"),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_visual_tests.rs"]
mod tests;
