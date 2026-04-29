//! Per-model visual metadata structs consumed by the GUI catalog layer.
//!
//! Lifted out of `lib.rs` (Phase 6 of issue #194). Visual config in a
//! business-logic crate is a known tension with the openrig-code-quality
//! "separation of concerns" rule (1b) — Phase 4b will eventually move
//! these to the GUI layer. Until then they stay here so adding a new
//! model still compiles.

/// Describes the position and range of a single knob overlay on the panel SVG.
#[derive(Debug, Clone, Copy)]
pub struct KnobLayoutEntry {
    pub param_key: &'static str,
    pub svg_cx: f32,
    pub svg_cy: f32,
    pub svg_r: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

/// Visual metadata for a model, used by the GUI catalog layer.
#[derive(Debug, Clone, Copy)]
pub struct ModelVisualData {
    pub brand: &'static str,
    pub type_label: &'static str,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [KnobLayoutEntry],
}
