use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "marshall",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0xb8, 0x98, 0x40],
            panel_text: [0x5a, 0x4a, 0x20],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
    }]
}
