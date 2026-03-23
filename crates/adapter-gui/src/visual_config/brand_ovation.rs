use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "ovation",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1E, 0x1E, 0x2A],
            panel_text: [0xC0, 0xC0, 0xD0],
            brand_strip_bg: [0x10, 0x10, 0x16],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
