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
    fn cry_classic_pinned() {
        let o = model_color_override("cry_classic").unwrap();
        assert_eq!(o.panel_bg, Some([0x34, 0x24, 0x1a]));
    }
    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("not_a_wah").is_none());
    }
}
