use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entry() -> VisualConfigEntry {
    VisualConfigEntry {
        brand: "",
        model_id: Some("eq_three_band_basic"),
        config: ModelVisualConfig {
            panel_bg: [0x24, 0x2c, 0x34],
            panel_text: [0x88, 0xa0, 0xc0],
            brand_strip_bg: [0x16, 0x1c, 0x22],
            model_font: "Orbitron",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }
}
