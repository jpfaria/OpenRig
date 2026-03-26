use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "gibson",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x8B, 0x6B, 0x3D],
            panel_text: [0xF4, 0xF0, 0xE8],
            brand_strip_bg: [0x1A, 0x1A, 0x1A],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
