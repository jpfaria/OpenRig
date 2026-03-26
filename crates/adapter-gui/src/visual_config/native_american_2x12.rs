use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("american_2x12"),
        config: ModelVisualConfig {
            panel_bg: [0x28, 0x2c, 0x30],
            panel_text: [0x80, 0x90, 0xa0],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
