use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("modulated_delay"),
        config: ModelVisualConfig {
            panel_bg: [0x2a, 0x1a, 0x3a],
            panel_text: [0xb0, 0x90, 0xd0],
            brand_strip_bg: [0x18, 0x10, 0x24],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
