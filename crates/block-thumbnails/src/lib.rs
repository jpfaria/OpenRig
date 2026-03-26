include!(concat!(env!("OUT_DIR"), "/generated_thumbnails.rs"));

/// Returns the PNG bytes for a specific model thumbnail.
/// Fallback chain: exact (effect_type, model_id) → (effect_type, "_default") → None
pub fn thumbnail_png(effect_type: &str, model_id: &str) -> Option<&'static [u8]> {
    THUMBNAILS
        .iter()
        .find(|(t, m, _)| *t == effect_type && *m == model_id)
        .map(|(_, _, bytes)| *bytes)
        .or_else(|| {
            THUMBNAILS
                .iter()
                .find(|(t, m, _)| *t == effect_type && *m == "_default")
                .map(|(_, _, bytes)| *bytes)
        })
}
