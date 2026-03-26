use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("tweed_breakup"),
        config: ModelVisualConfig {
            panel_bg: [0x38, 0x30, 0x22],
            panel_text: [0x80, 0x90, 0xa0],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "Permanent Marker",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
