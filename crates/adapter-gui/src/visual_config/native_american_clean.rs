use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("american_clean"),
        config: ModelVisualConfig {
            panel_bg: [0x2a, 0x33, 0x38],
            panel_text: [0x80, 0x90, 0xa0],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
