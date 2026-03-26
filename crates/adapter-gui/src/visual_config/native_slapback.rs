use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("slapback"),
        config: ModelVisualConfig {
            panel_bg: [0x30, 0x2a, 0x20],
            panel_text: [0xc0, 0xa8, 0x80],
            brand_strip_bg: [0x1e, 0x18, 0x12],
            model_font: "Permanent Marker",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
