use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("compressor_studio_clean"),
        config: ModelVisualConfig {
            panel_bg: [0x28, 0x30, 0x2a],
            panel_text: [0x90, 0xb0, 0x90],
            brand_strip_bg: [0x18, 0x20, 0x1a],
            model_font: "Orbitron",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
