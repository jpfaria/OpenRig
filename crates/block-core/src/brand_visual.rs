//! Brand-level visual catalog. Phase 4b of issue #194.
//!
//! ## Why this lives in `block-core`
//!
//! The "brand" is a category many models share — Marshall amps, Boss
//! pedals, Fender bodies. Putting brand colors here means **one source
//! of truth per brand**, not duplicated across every Marshall amp's
//! `MODEL_DEFINITION`. Two crates would otherwise pin the same hex
//! triple, so this is exactly the "shared used by 2+ features" case
//! that earns a place in `block-core`.
//!
//! Per-model overrides do NOT live here — they belong to the owning
//! block-* crate so a model's visual stays with the model itself
//! (per the issue #194 acceptance: "modelos independentes").

/// Static color scheme for a panel. Stored as RGB byte triples + a
/// font name. Layout offsets are screen-space ratios in `[-1.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelColorScheme {
    pub panel_bg: [u8; 3],
    pub panel_text: [u8; 3],
    pub brand_strip_bg: [u8; 3],
    pub model_font: &'static str,
    pub photo_offset_x: f32,
    pub photo_offset_y: f32,
}

impl ModelColorScheme {
    /// Project-wide neutral default. Used when neither brand nor model
    /// declare a scheme.
    pub const DEFAULT: ModelColorScheme = ModelColorScheme {
        panel_bg: [0x2c, 0x2e, 0x34],
        panel_text: [0x80, 0x90, 0xa0],
        brand_strip_bg: [0x1a, 0x1a, 0x1a],
        model_font: "",
        photo_offset_x: 0.0,
        photo_offset_y: 0.0,
    };
}

/// Per-model override. All fields optional — `None` means "inherit
/// from the brand (or default if no brand)".
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ModelColorOverride {
    pub panel_bg: Option<[u8; 3]>,
    pub panel_text: Option<[u8; 3]>,
    pub brand_strip_bg: Option<[u8; 3]>,
    pub model_font: Option<&'static str>,
    pub photo_offset_x: Option<f32>,
    pub photo_offset_y: Option<f32>,
}

/// Compose final scheme: start from default, layer brand on top, then
/// layer per-model override on top. Each layer can be `None` to skip.
pub fn compose(
    brand: Option<ModelColorScheme>,
    model_override: Option<ModelColorOverride>,
) -> ModelColorScheme {
    let mut out = brand.unwrap_or(ModelColorScheme::DEFAULT);
    if let Some(o) = model_override {
        if let Some(v) = o.panel_bg {
            out.panel_bg = v;
        }
        if let Some(v) = o.panel_text {
            out.panel_text = v;
        }
        if let Some(v) = o.brand_strip_bg {
            out.brand_strip_bg = v;
        }
        if let Some(v) = o.model_font {
            out.model_font = v;
        }
        if let Some(v) = o.photo_offset_x {
            out.photo_offset_x = v;
        }
        if let Some(v) = o.photo_offset_y {
            out.photo_offset_y = v;
        }
    }
    out
}

/// Lookup a brand's default color scheme. Returns `None` for unknown
/// brand strings (caller falls back to `ModelColorScheme::DEFAULT`).
pub fn brand_colors(brand: &str) -> Option<ModelColorScheme> {
    BRAND_TABLE
        .iter()
        .find(|(b, _)| *b == brand)
        .map(|(_, s)| *s)
}

/// All registered brands. Adding a new brand = one row here. No
/// per-model row, ever — those go in the owning block-* crate.
///
/// Values are bit-exact with the legacy `adapter-gui/src/visual_config/brand_*.rs`
/// (Phase 4b — September 2026 audit). Any deviation breaks visual A/B.
const BRAND_TABLE: &[(&str, ModelColorScheme)] = &[
    (
        "bogner",
        ModelColorScheme {
            panel_bg: [0x28, 0x18, 0x24],
            panel_text: [0xc0, 0xa0, 0xb8],
            brand_strip_bg: [0x18, 0x0c, 0x16],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
    ),
    (
        "boss",
        ModelColorScheme {
            panel_bg: [0x1a, 0x3a, 0x6a],
            panel_text: [0xc0, 0xd0, 0xe8],
            brand_strip_bg: [0x10, 0x20, 0x40],
            model_font: "Orbitron",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
    ),
    (
        "collings",
        ModelColorScheme {
            panel_bg: [0x6A, 0x5A, 0x3A],
            panel_text: [0xF0, 0xE8, 0xD8],
            brand_strip_bg: [0x2A, 0x22, 0x16],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "cort",
        ModelColorScheme {
            panel_bg: [0x34, 0x2A, 0x22],
            panel_text: [0xE0, 0xD0, 0xC0],
            brand_strip_bg: [0x1E, 0x16, 0x10],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "diezel",
        ModelColorScheme {
            panel_bg: [0x28, 0x28, 0x28],
            panel_text: [0xd0, 0xd0, 0xd0],
            brand_strip_bg: [0x14, 0x14, 0x14],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "dumble",
        ModelColorScheme {
            panel_bg: [0x9a, 0x8a, 0x6a],
            panel_text: [0x2a, 0x2a, 0x1a],
            brand_strip_bg: [0x3a, 0x30, 0x20],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "emerald",
        ModelColorScheme {
            panel_bg: [0x1A, 0x3A, 0x3A],
            panel_text: [0xC0, 0xE0, 0xE0],
            brand_strip_bg: [0x0E, 0x1E, 0x1E],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "evh",
        ModelColorScheme {
            panel_bg: [0x1a, 0x1a, 0x1a],
            panel_text: [0xe0, 0xe0, 0xe0],
            brand_strip_bg: [0x10, 0x10, 0x10],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "fender",
        ModelColorScheme {
            panel_bg: [0x8a, 0x6a, 0x3a],
            panel_text: [0xf0, 0xe8, 0xd8],
            brand_strip_bg: [0x3a, 0x2a, 0x1a],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "gibson",
        ModelColorScheme {
            panel_bg: [0x8B, 0x6B, 0x3D],
            panel_text: [0xF4, 0xF0, 0xE8],
            brand_strip_bg: [0x1A, 0x1A, 0x1A],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "godin",
        ModelColorScheme {
            panel_bg: [0x3A, 0x2A, 0x1A],
            panel_text: [0xE0, 0xD0, 0xC0],
            brand_strip_bg: [0x1E, 0x16, 0x0E],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "guild",
        ModelColorScheme {
            panel_bg: [0x7A, 0x6A, 0x4A],
            panel_text: [0xF0, 0xE8, 0xD8],
            brand_strip_bg: [0x30, 0x28, 0x1A],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "ibanez",
        ModelColorScheme {
            panel_bg: [0x1a, 0x5c, 0x2a],
            panel_text: [0xc0, 0xe0, 0xc0],
            brand_strip_bg: [0x12, 0x3a, 0x1a],
            model_font: "Permanent Marker",
            photo_offset_x: 0.0,
            photo_offset_y: -0.3,
        },
    ),
    (
        "jhs",
        ModelColorScheme {
            panel_bg: [0x8a, 0x2a, 0x2a],
            panel_text: [0xf0, 0xe0, 0xe0],
            brand_strip_bg: [0x4a, 0x14, 0x14],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "lakewood",
        ModelColorScheme {
            panel_bg: [0x5A, 0x7A, 0x5A],
            panel_text: [0xE0, 0xF0, 0xE0],
            brand_strip_bg: [0x22, 0x30, 0x22],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "lowden",
        ModelColorScheme {
            panel_bg: [0x8A, 0x7A, 0x5A],
            panel_text: [0xF0, 0xE8, 0xD8],
            brand_strip_bg: [0x3A, 0x30, 0x22],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "marshall",
        ModelColorScheme {
            panel_bg: [0xb8, 0x98, 0x40],
            panel_text: [0x5a, 0x4a, 0x20],
            brand_strip_bg: [0x1a, 0x1a, 0x1a],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: -0.2,
        },
    ),
    (
        "martin",
        ModelColorScheme {
            panel_bg: [0xA0, 0x82, 0x50],
            panel_text: [0xF4, 0xF0, 0xE8],
            brand_strip_bg: [0x3D, 0x2B, 0x1F],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "mesa",
        ModelColorScheme {
            panel_bg: [0x1a, 0x2a, 0x1a],
            panel_text: [0xc0, 0xe0, 0xc0],
            brand_strip_bg: [0x0e, 0x18, 0x0e],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "morris",
        ModelColorScheme {
            panel_bg: [0x3A, 0x3A, 0x44],
            panel_text: [0xD0, 0xD0, 0xE0],
            brand_strip_bg: [0x1A, 0x1A, 0x22],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "ovation",
        ModelColorScheme {
            panel_bg: [0x1E, 0x1E, 0x2A],
            panel_text: [0xC0, 0xC0, 0xD0],
            brand_strip_bg: [0x10, 0x10, 0x16],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "peavey",
        ModelColorScheme {
            panel_bg: [0x1a, 0x1a, 0x28],
            panel_text: [0xc0, 0xc0, 0xe0],
            brand_strip_bg: [0x10, 0x10, 0x1a],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "rainsong",
        ModelColorScheme {
            panel_bg: [0x2A, 0x3A, 0x4A],
            panel_text: [0xD0, 0xE0, 0xF0],
            brand_strip_bg: [0x14, 0x1E, 0x28],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "roland",
        ModelColorScheme {
            panel_bg: [0x1a, 0x1a, 0x1e],
            panel_text: [0xa0, 0xa8, 0xb8],
            brand_strip_bg: [0x10, 0x10, 0x14],
            model_font: "Dancing Script",
            photo_offset_x: 0.0,
            photo_offset_y: -0.3,
        },
    ),
    (
        "santa_cruz",
        ModelColorScheme {
            panel_bg: [0x2A, 0x2A, 0x2A],
            panel_text: [0xE0, 0xE0, 0xE0],
            brand_strip_bg: [0x14, 0x14, 0x14],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "suhr",
        ModelColorScheme {
            panel_bg: [0x1A, 0x1A, 0x1A],
            panel_text: [0xE0, 0xE0, 0xE0],
            brand_strip_bg: [0x10, 0x10, 0x10],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "takamine",
        ModelColorScheme {
            panel_bg: [0x2A, 0x34, 0x2A],
            panel_text: [0xD0, 0xE0, 0xD0],
            brand_strip_bg: [0x14, 0x1E, 0x14],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "taylor",
        ModelColorScheme {
            panel_bg: [0x8C, 0x5A, 0x3A],
            panel_text: [0xF4, 0xF0, 0xE8],
            brand_strip_bg: [0x2A, 0x1A, 0x12],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "vox",
        ModelColorScheme {
            panel_bg: [0x1a, 0x1a, 0x2a],
            panel_text: [0xaa, 0xbb, 0xcc],
            brand_strip_bg: [0x0a, 0x15, 0x20],
            model_font: "",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
    (
        "yamaha",
        ModelColorScheme {
            panel_bg: [0x28, 0x28, 0x38],
            panel_text: [0xD0, 0xD0, 0xE0],
            brand_strip_bg: [0x14, 0x14, 0x1E],
            model_font: "Inter",
            photo_offset_x: 0.0,
            photo_offset_y: 0.0,
        },
    ),
];

#[cfg(test)]
#[path = "brand_visual_tests.rs"]
mod tests;
