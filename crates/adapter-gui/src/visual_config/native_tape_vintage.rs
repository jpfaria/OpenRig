use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("tape_vintage"),
        config: ModelVisualConfig {
            panel_bg: [0x38, 0x28, 0x18],
            panel_text: [0xd0, 0xb8, 0x90],
            brand_strip_bg: [0x22, 0x18, 0x0e],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
