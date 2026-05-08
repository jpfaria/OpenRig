//! Per-model visual color overrides for native preamps owned by this crate.
//! Phase 4b of issue #194 — see `block-amp::model_visual` for rationale.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "american_clean" => Some(ModelColorOverride {
            panel_bg: Some([0x2a, 0x33, 0x38]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Dancing Script"),
        }),
        "brit_crunch" => Some(ModelColorOverride {
            panel_bg: Some([0x34, 0x2e, 0x28]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Permanent Marker"),
        }),
        "modern_high_gain" => Some(ModelColorOverride {
            panel_bg: Some([0x2a, 0x24, 0x34]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Orbitron"),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_visual_tests.rs"]
mod tests;
