use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "cort",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x34, 0x2A, 0x22],
            panel_text: [0xE0, 0xD0, 0xC0],
            brand_strip_bg: [0x1E, 0x16, 0x10],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
