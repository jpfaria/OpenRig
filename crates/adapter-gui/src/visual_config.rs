/// Visual configuration for block models.
///
/// All colors and fonts live here — never in the business‑logic crates.
pub struct ModelVisualConfig {
    pub panel_bg: [u8; 3],
    pub panel_text: [u8; 3],
    pub brand_strip_bg: [u8; 3],
    pub model_font: &'static str,
}

pub fn visual_config_for_model(brand: &str, model_id: &str) -> ModelVisualConfig {
    match brand {
        "marshall" => ModelVisualConfig {
            panel_bg: [0xb8, 0x98, 0x40],
            panel_text: [0x5a, 0x4a, 0x20],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "",
        },
        "ibanez" => ModelVisualConfig {
            panel_bg: [0x1a, 0x5c, 0x2a],
            panel_text: [0xc0, 0xe0, 0xc0],
            brand_strip_bg: [0x12, 0x3a, 0x1a],
            model_font: "Permanent Marker",
        },
        "boss" => ModelVisualConfig {
            panel_bg: [0x1a, 0x3a, 0x6a],
            panel_text: [0xc0, 0xd0, 0xe8],
            brand_strip_bg: [0x10, 0x20, 0x40],
            model_font: "Orbitron",
        },
        "roland" => ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x1e],
            panel_text: [0xa0, 0xa8, 0xb8],
            brand_strip_bg: [0x10, 0x10, 0x14],
            model_font: "Dancing Script",
        },
        "bogner" => ModelVisualConfig {
            panel_bg: [0x28, 0x18, 0x24],
            panel_text: [0xc0, 0xa0, 0xb8],
            brand_strip_bg: [0x18, 0x0c, 0x16],
            model_font: "Dancing Script",
        },
        "vox" => ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x2a],
            panel_text: [0xaa, 0xbb, 0xcc],
            brand_strip_bg: [0x0a, 0x15, 0x20],
            model_font: "",
        },
        _ => {
            // Native models — per-model colors
            match model_id {
                "american_clean" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x33, 0x38],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Dancing Script",
                },
                "brit_crunch" => ModelVisualConfig {
                    panel_bg: [0x34, 0x2e, 0x28],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Permanent Marker",
                },
                "modern_high_gain" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x24, 0x34],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Orbitron",
                },
                "blackface_clean" => ModelVisualConfig {
                    panel_bg: [0x28, 0x30, 0x38],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Dancing Script",
                },
                "tweed_breakup" => ModelVisualConfig {
                    panel_bg: [0x38, 0x30, 0x22],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Permanent Marker",
                },
                "chime" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x34, 0x2a],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Orbitron",
                },
                "american_2x12" => ModelVisualConfig {
                    panel_bg: [0x28, 0x2c, 0x30],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "",
                },
                "brit_4x12" => ModelVisualConfig {
                    panel_bg: [0x2c, 0x28, 0x24],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "",
                },
                "vintage_1x12" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x2a, 0x2e],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "",
                },
                _ => ModelVisualConfig {
                    panel_bg: [0x2c, 0x2e, 0x34],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "",
                },
            }
        }
    }
}
