use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "ibanez",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x5c, 0x2a],
            panel_text: [0xc0, 0xe0, 0xc0],
            brand_strip_bg: [0x12, 0x3a, 0x1a],
            model_font: "Permanent Marker",
            photo_offset_x: 0.0,
            photo_offset_y: -0.3,
        },
    }]
}
