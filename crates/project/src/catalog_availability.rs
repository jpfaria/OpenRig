//! Responsibility: says whether a model has a backend on this machine.

/// Returns true if the model has a usable backend on the current platform.
pub fn is_model_available(effect_type: &str, model_id: &str) -> bool {
    use block_core::*;
    match effect_type {
        EFFECT_TYPE_REVERB => block_reverb::is_reverb_model_available(model_id),
        EFFECT_TYPE_DELAY => block_delay::is_delay_model_available(model_id),
        EFFECT_TYPE_MODULATION => block_mod::is_mod_model_available(model_id),
        EFFECT_TYPE_FILTER => block_filter::is_filter_model_available(model_id),
        EFFECT_TYPE_DYNAMICS => block_dyn::is_dyn_model_available(model_id),
        EFFECT_TYPE_GAIN => block_gain::is_gain_model_available(model_id),
        EFFECT_TYPE_PITCH => block_pitch::is_pitch_model_available(model_id),
        _ => true,
    }
}
