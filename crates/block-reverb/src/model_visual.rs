//! Per-model visual color overrides for native reverbs owned by this crate.
//! Phase 4b of issue #194.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "plate_foundation" => Some(ModelColorOverride {
            panel_bg: Some([0x20, 0x28, 0x34]),
            panel_text: Some([0x90, 0xa8, 0xc8]),
            brand_strip_bg: Some([0x14, 0x1a, 0x22]),
            model_font: Some("Dancing Script"),
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
    fn plate_foundation_pinned() {
        let o = model_color_override("plate_foundation").unwrap();
        assert_eq!(o.panel_bg, Some([0x20, 0x28, 0x34]));
    }
    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("hall").is_none());
    }
}
