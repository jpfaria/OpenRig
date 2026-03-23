use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "roland",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x1e],
            panel_text: [0xa0, 0xa8, 0xb8],
            brand_strip_bg: [0x10, 0x10, 0x14],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: -0.3,
        },
    }]
}
