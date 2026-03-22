/// Visual configuration for block models.
///
/// All colors and fonts live here — never in the business‑logic crates.
pub struct ModelVisualConfig {
    pub panel_bg: [u8; 3],
    pub panel_text: [u8; 3],
    pub brand_strip_bg: [u8; 3],
    pub model_font: &'static str,
    /// Photo offset X: -1.0 = shift left, 0.0 = center, 1.0 = shift right
    pub photo_offset_x: f32,
    /// Photo offset Y: -1.0 = shift up, 0.0 = center, 1.0 = shift down
    pub photo_offset_y: f32,
}

pub fn visual_config_for_model(brand: &str, model_id: &str) -> ModelVisualConfig {
    match brand {
        "marshall" => ModelVisualConfig {
            panel_bg: [0xb8, 0x98, 0x40],
            panel_text: [0x5a, 0x4a, 0x20],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
        "ibanez" => ModelVisualConfig {
            panel_bg: [0x1a, 0x5c, 0x2a],
            panel_text: [0xc0, 0xe0, 0xc0],
            brand_strip_bg: [0x12, 0x3a, 0x1a],
            model_font: "Permanent Marker",
            photo_offset_x: 0.0,
            photo_offset_y: -0.3,
        },
        "boss" => ModelVisualConfig {
            panel_bg: [0x1a, 0x3a, 0x6a],
            panel_text: [0xc0, 0xd0, 0xe8],
            brand_strip_bg: [0x10, 0x20, 0x40],
            model_font: "Orbitron",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
        "roland" => ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x1e],
            panel_text: [0xa0, 0xa8, 0xb8],
            brand_strip_bg: [0x10, 0x10, 0x14],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: -0.3,
        },
        "bogner" => ModelVisualConfig {
            panel_bg: [0x28, 0x18, 0x24],
            panel_text: [0xc0, 0xa0, 0xb8],
            brand_strip_bg: [0x18, 0x0c, 0x16],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
        "vox" => ModelVisualConfig {
            panel_bg: [0x1a, 0x1a, 0x2a],
            panel_text: [0xaa, 0xbb, 0xcc],
            brand_strip_bg: [0x0a, 0x15, 0x20],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
        _ => {
            // Native models — per-model colors
            match model_id {
                "american_clean" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x33, 0x38],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "brit_crunch" => ModelVisualConfig {
                    panel_bg: [0x34, 0x2e, 0x28],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Permanent Marker",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "modern_high_gain" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x24, 0x34],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "blackface_clean" => ModelVisualConfig {
                    panel_bg: [0x28, 0x30, 0x38],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "tweed_breakup" => ModelVisualConfig {
                    panel_bg: [0x38, 0x30, 0x22],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Permanent Marker",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "chime" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x34, 0x2a],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "american_2x12" => ModelVisualConfig {
                    panel_bg: [0x28, 0x2c, 0x30],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "brit_4x12" => ModelVisualConfig {
                    panel_bg: [0x2c, 0x28, 0x24],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Permanent Marker",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "vintage_1x12" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x2a, 0x2e],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                // Delays
                "analog_warm" => ModelVisualConfig {
                    panel_bg: [0x3a, 0x2a, 0x1a],
                    panel_text: [0xd0, 0xb0, 0x80],
                    brand_strip_bg: [0x20, 0x18, 0x10],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "digital_clean" => ModelVisualConfig {
                    panel_bg: [0x1a, 0x28, 0x3a],
                    panel_text: [0x80, 0xb0, 0xe0],
                    brand_strip_bg: [0x10, 0x18, 0x24],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "modulated_delay" => ModelVisualConfig {
                    panel_bg: [0x2a, 0x1a, 0x3a],
                    panel_text: [0xb0, 0x90, 0xd0],
                    brand_strip_bg: [0x18, 0x10, 0x24],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "reverse" => ModelVisualConfig {
                    panel_bg: [0x1a, 0x1a, 0x30],
                    panel_text: [0x90, 0x90, 0xd0],
                    brand_strip_bg: [0x10, 0x10, 0x20],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "slapback" => ModelVisualConfig {
                    panel_bg: [0x30, 0x2a, 0x20],
                    panel_text: [0xc0, 0xa8, 0x80],
                    brand_strip_bg: [0x1e, 0x18, 0x12],
                    model_font: "Permanent Marker",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "tape_vintage" => ModelVisualConfig {
                    panel_bg: [0x38, 0x28, 0x18],
                    panel_text: [0xd0, 0xb8, 0x90],
                    brand_strip_bg: [0x22, 0x18, 0x0e],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                // Reverb
                "plate_foundation" => ModelVisualConfig {
                    panel_bg: [0x20, 0x28, 0x34],
                    panel_text: [0x90, 0xa8, 0xc8],
                    brand_strip_bg: [0x14, 0x1a, 0x22],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                // Dynamics
                "compressor_studio_clean" => ModelVisualConfig {
                    panel_bg: [0x28, 0x30, 0x2a],
                    panel_text: [0x90, 0xb0, 0x90],
                    brand_strip_bg: [0x18, 0x20, 0x1a],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                "gate_basic" => ModelVisualConfig {
                    panel_bg: [0x30, 0x28, 0x28],
                    panel_text: [0xb0, 0x90, 0x90],
                    brand_strip_bg: [0x20, 0x18, 0x18],
                    model_font: "Permanent Marker",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                // Filter
                "eq_three_band_basic" => ModelVisualConfig {
                    panel_bg: [0x24, 0x2c, 0x34],
                    panel_text: [0x88, 0xa0, 0xc0],
                    brand_strip_bg: [0x16, 0x1c, 0x22],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                // Wah
                "cry_classic" => ModelVisualConfig {
                    panel_bg: [0x34, 0x24, 0x1a],
                    panel_text: [0xc8, 0xa0, 0x70],
                    brand_strip_bg: [0x22, 0x16, 0x0e],
                    model_font: "Permanent Marker",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                // Mod
                "tremolo_sine" => ModelVisualConfig {
                    panel_bg: [0x1a, 0x30, 0x30],
                    panel_text: [0x80, 0xc0, 0xc0],
                    brand_strip_bg: [0x10, 0x20, 0x20],
                    model_font: "Dancing Script",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                // Tuner
                "tuner_chromatic" => ModelVisualConfig {
                    panel_bg: [0x1a, 0x1a, 0x20],
                    panel_text: [0x90, 0x90, 0xa8],
                    brand_strip_bg: [0x10, 0x10, 0x16],
                    model_font: "Orbitron",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
                _ => ModelVisualConfig {
                    panel_bg: [0x2c, 0x2e, 0x34],
                    panel_text: [0x80, 0x90, 0xa0],
                    brand_strip_bg: [0x1a, 0x1a, 0x1a],
                    model_font: "",
                    photo_offset_x: 0.0,
                    photo_offset_y: 0.0,
                },
            }
        }
    }
}
