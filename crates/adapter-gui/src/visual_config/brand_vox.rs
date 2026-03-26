use super::{ModelVisualConfig, VisualConfigEntry};

pub fn entries() -> Vec<VisualConfigEntry> {
    vec![VisualConfigEntry {
        brand: "vox",
        model_id: None,
        config: ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x2a],
            panel_text: [0xaa, 0xbb, 0xcc],
            brand_strip_bg: [0x0a, 0x15, 0x20],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    }]
}
