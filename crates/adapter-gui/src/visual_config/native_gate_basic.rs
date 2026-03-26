use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("gate_basic"),
        config: ModelVisualConfig {
            panel_bg: [0x30, 0x28, 0x28],
            panel_text: [0xb0, 0x90, 0x90],
            brand_strip_bg: [0x20, 0x18, 0x18],
            model_font: "Permanent Marker",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
