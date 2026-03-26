use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "rainsong",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x2A, 0x3A, 0x4A],
            panel_text: [0xD0, 0xE0, 0xF0],
            brand_strip_bg: [0x14, 0x1E, 0x28],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
