use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "mesa",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x2a, 0x1a],
            panel_text: [0xc0, 0xe0, 0xc0],
            brand_strip_bg: [0x0e, 0x18, 0x0e],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
