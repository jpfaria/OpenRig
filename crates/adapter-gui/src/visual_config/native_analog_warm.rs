use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("analog_warm"),
        config: ModelVisualConfig {
            panel_bg: [0x3a, 0x2a, 0x1a],
            panel_text: [0xd0, 0xb0, 0x80],
            brand_strip_bg: [0x20, 0x18, 0x10],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
