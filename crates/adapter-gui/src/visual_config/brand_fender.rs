use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "fender",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x8a, 0x6a, 0x3a],
            panel_text: [0xf0, 0xe8, 0xd8],
            brand_strip_bg: [0x3a, 0x2a, 0x1a],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
