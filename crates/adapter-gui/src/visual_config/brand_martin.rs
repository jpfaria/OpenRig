use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "martin",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0xA0, 0x82, 0x50],
            panel_text: [0xF4, 0xF0, 0xE8],
            brand_strip_bg: [0x3D, 0x2B, 0x1F],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
