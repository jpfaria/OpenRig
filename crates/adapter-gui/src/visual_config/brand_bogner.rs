use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "bogner",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x28, 0x18, 0x24],
            panel_text: [0xc0, 0xa0, 0xb8],
            brand_strip_bg: [0x18, 0x0c, 0x16],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
    }]
}
