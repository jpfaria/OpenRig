use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("digital_clean"),
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x28, 0x3a],
            panel_text: [0x80, 0xb0, 0xe0],
            brand_strip_bg: [0x10, 0x18, 0x24],
            model_font: "Orbitron",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
