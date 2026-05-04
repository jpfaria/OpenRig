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
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "brit_crunch" => Some(ModelColorOverride {
            panel_bg: Some([0x34, 0x2e, 0x28]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Permanent Marker"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "modern_high_gain" => Some(ModelColorOverride {
            panel_bg: Some([0x2a, 0x24, 0x34]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Orbitron"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models_return_some() {
        assert!(model_color_override("american_clean").is_some());
        assert!(model_color_override("brit_crunch").is_some());
        assert!(model_color_override("modern_high_gain").is_some());
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("nam_marshall_plexi").is_none());
    }
}
