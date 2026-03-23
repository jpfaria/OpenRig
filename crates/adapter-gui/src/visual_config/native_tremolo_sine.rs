use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("tremolo_sine"),
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x30, 0x30],
            panel_text: [0x80, 0xc0, 0xc0],
            brand_strip_bg: [0x10, 0x20, 0x20],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
