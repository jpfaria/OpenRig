use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "dumble",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x9a, 0x8a, 0x6a],
            panel_text: [0x2a, 0x2a, 0x1a],
            brand_strip_bg: [0x3a, 0x30, 0x20],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
