//! Per-model visual color overrides for native modulation owned by this crate.
//! Phase 4b of issue #194.

use block_core::ModelColorOverride;

pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "tremolo_sine" => Some(ModelColorOverride {
            panel_bg: Some([0x1a, 0x30, 0x30]),
            panel_text: Some([0x80, 0xc0, 0xc0]),
            brand_strip_bg: Some([0x10, 0x20, 0x20]),
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
    fn tremolo_sine_pinned() {
        let o = model_color_override("tremolo_sine").unwrap();
        assert_eq!(o.panel_bg, Some([0x1a, 0x30, 0x30]));
    }
    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("classic_chorus").is_none());
    }
}
