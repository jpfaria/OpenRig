use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("chime"),
        config: ModelVisualConfig {
            panel_bg: [0x2a, 0x34, 0x2a],
            panel_text: [0x80, 0x90, 0xa0],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "Orbitron",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
