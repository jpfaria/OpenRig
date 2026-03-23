use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "jhs",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x8a, 0x2a, 0x2a],
            panel_text: [0xf0, 0xe0, 0xe0],
            brand_strip_bg: [0x4a, 0x14, 0x14],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
