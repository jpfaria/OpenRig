use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "boss",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x3a, 0x6a],
            panel_text: [0xc0, 0xd0, 0xe8],
            brand_strip_bg: [0x10, 0x20, 0x40],
            model_font: "Orbitron",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
    }]
}
