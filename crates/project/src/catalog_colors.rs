//! Responsibility: resolves the colour scheme a model is drawn with.

use block_core::{ModelColorOverride, ModelColorScheme};

use crate::catalog_registry::block_registry;

/// Per-effect-type dispatch: returns the color override declared by the
/// owning block-* crate for `model_id`, or `None` if the model has no
/// override (the brand fallback applies).
pub fn model_color_override(effect_type: &str, model_id: &str) -> Option<ModelColorOverride> {
    block_registry()
        .into_iter()
        .find(|e| e.effect_type == effect_type)
        .and_then(|e| (e.model_color_override)(model_id))
}

/// Resolve the final color scheme for a model: brand colors (centralized
/// in `block_core::brand_visual`) layered with the model's per-crate
/// override, falling back to `ModelColorScheme::DEFAULT` when neither
/// brand nor override is registered.
///
/// This is the public surface adapter-gui calls during rendering,
/// replacing the legacy `adapter-gui/src/visual_config/` lookup.
pub fn resolve_color_scheme(effect_type: &str, brand: &str, model_id: &str) -> ModelColorScheme {
    let brand_scheme = block_core::brand_colors(brand);
    let override_ = model_color_override(effect_type, model_id);
    block_core::compose(brand_scheme, override_)
}
