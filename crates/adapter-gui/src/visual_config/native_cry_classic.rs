use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("cry_classic"),
        config: ModelVisualConfig {
            panel_bg: [0x34, 0x24, 0x1a],
            panel_text: [0xc8, 0xa0, 0x70],
            brand_strip_bg: [0x22, 0x16, 0x0e],
            model_font: "Permanent Marker",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
