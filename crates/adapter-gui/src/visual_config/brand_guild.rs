use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "guild",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x7A, 0x6A, 0x4A],
            panel_text: [0xF0, 0xE8, 0xD8],
            brand_strip_bg: [0x30, 0x28, 0x1A],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
