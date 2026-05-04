//! Per-model visual color overrides for native cabs owned by this crate.
//! Phase 4b of issue #194 — see `block-amp::model_visual` for rationale.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "american_2x12" => Some(ModelColorOverride {
            panel_bg: Some([0x28, 0x2c, 0x30]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Dancing Script"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "brit_4x12" => Some(ModelColorOverride {
            panel_bg: Some([0x2c, 0x28, 0x24]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Permanent Marker"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "vintage_1x12" => Some(ModelColorOverride {
            panel_bg: Some([0x2a, 0x2a, 0x2e]),
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
        assert!(model_color_override("american_2x12").is_some());
        assert!(model_color_override("brit_4x12").is_some());
        assert!(model_color_override("vintage_1x12").is_some());
    }
    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("g12t_75_4x12").is_none());
    }
}
