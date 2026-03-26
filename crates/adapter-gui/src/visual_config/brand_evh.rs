use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "evh",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x1a],
            panel_text: [0xe0, 0xe0, 0xe0],
            brand_strip_bg: [0x10, 0x10, 0x10],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
