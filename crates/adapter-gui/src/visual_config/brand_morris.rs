use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "morris",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x3A, 0x3A, 0x44],
            panel_text: [0xD0, 0xD0, 0xE0],
            brand_strip_bg: [0x1A, 0x1A, 0x22],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
