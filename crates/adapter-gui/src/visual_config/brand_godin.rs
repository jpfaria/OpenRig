use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "godin",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x3A, 0x2A, 0x1A],
            panel_text: [0xE0, 0xD0, 0xC0],
            brand_strip_bg: [0x1E, 0x16, 0x0E],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
