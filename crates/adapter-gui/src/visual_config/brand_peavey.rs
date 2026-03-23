use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "peavey",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x28],
            panel_text: [0xc0, 0xc0, 0xe0],
            brand_strip_bg: [0x10, 0x10, 0x1a],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
