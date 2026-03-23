use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "taylor",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x8C, 0x5A, 0x3A],
            panel_text: [0xF4, 0xF0, 0xE8],
            brand_strip_bg: [0x2A, 0x1A, 0x12],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
