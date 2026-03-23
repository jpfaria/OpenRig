use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "emerald",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1A, 0x3A, 0x3A],
            panel_text: [0xC0, 0xE0, 0xE0],
            brand_strip_bg: [0x0E, 0x1E, 0x1E],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
