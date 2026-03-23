use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "diezel",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x28, 0x28, 0x28],
            panel_text: [0xd0, 0xd0, 0xd0],
            brand_strip_bg: [0x14, 0x14, 0x14],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
