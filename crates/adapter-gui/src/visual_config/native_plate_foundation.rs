use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("plate_foundation"),
        config: ModelVisualConfig {
            panel_bg: [0x20, 0x28, 0x34],
            panel_text: [0x90, 0xa8, 0xc8],
            brand_strip_bg: [0x14, 0x1a, 0x22],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
