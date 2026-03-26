use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "collings",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x6A, 0x5A, 0x3A],
            panel_text: [0xF0, 0xE8, 0xD8],
            brand_strip_bg: [0x2A, 0x22, 0x16],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
