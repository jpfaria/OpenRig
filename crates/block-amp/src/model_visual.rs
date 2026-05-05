//! Per-model visual color overrides for native amps owned by this crate.
//!
//! Phase 4b of issue #194 — visual data for a native model lives WITH the
//! model's owning block crate so adding a new model touches only this
//! crate, never `adapter-gui/src/visual_config/`.
//!
//! Branded models (NAM amps that declare a `brand:` value) inherit from
//! `block_core::brand_colors(brand)` and don't need an entry here.

use block_core::ModelColorOverride;

/// Returns the color override for a native model owned by `block-amp`,
/// or `None` if the model has no override (brand fallback applies).
///
/// Bytes are bit-exact with the legacy
/// `adapter-gui/src/visual_config/native_*.rs` (audited 2026-04-30).
pub fn model_color_override(model_id: &str) -> Option<ModelColorOverride> {
    match model_id {
        "blackface_clean" => Some(ModelColorOverride {
            panel_bg: Some([0x28, 0x30, 0x38]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Dancing Script"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "chime" => Some(ModelColorOverride {
            panel_bg: Some([0x2a, 0x34, 0x2a]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Orbitron"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        "tweed_breakup" => Some(ModelColorOverride {
            panel_bg: Some([0x38, 0x30, 0x22]),
            panel_text: Some([0x80, 0x90, 0xa0]),
            brand_strip_bg: Some([0x1a, 0x1a, 0x1a]),
            model_font: Some("Permanent Marker"),
            photo_offset_x: Some(0.0),
            photo_offset_y: Some(0.0),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_visual_tests.rs"]
mod tests;
