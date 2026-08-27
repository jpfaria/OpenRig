//! Responsibility: names the model a block family falls back to when the document omits it.
//!
//! Split out of `lib.rs` (#873). Each block crate owns its own list; this
//! file only asks each one for its first entry.
//!
//! #913: a family with no registered model yields an EMPTY default. Loading a
//! document must never panic on what it omits — `block-full-rig` ships zero
//! models today, so `type: full_rig` without `model:` used to abort the whole
//! load. An empty model reaches `find_model_definition`, which reports it as
//! the unsupported model it is.

fn first_supported(models: &[&str]) -> String {
    models.first().unwrap_or(&"").to_string()
}

pub(crate) fn default_delay_model() -> String {
    first_supported(block_delay::supported_models())
}

pub(crate) fn default_nam_model() -> String {
    first_supported(block_nam::supported_models())
}

pub(crate) fn default_preamp_model() -> String {
    first_supported(block_preamp::supported_models())
}

pub(crate) fn default_amp_model() -> String {
    first_supported(block_amp::supported_models())
}

pub(crate) fn default_full_rig_model() -> String {
    first_supported(block_full_rig::supported_models())
}

pub(crate) fn default_cab_model() -> String {
    first_supported(block_cab::supported_models())
}

pub(crate) fn default_body_model() -> String {
    first_supported(block_body::supported_models())
}

pub(crate) fn default_drive_model() -> String {
    first_supported(block_gain::supported_models())
}

pub(crate) fn default_reverb_model() -> String {
    first_supported(block_reverb::supported_models())
}

pub(crate) fn default_utility_model() -> String {
    first_supported(block_util::supported_models())
}

pub(crate) fn default_dynamics_model() -> String {
    first_supported(block_dyn::supported_models())
}

pub(crate) fn default_filter_model() -> String {
    first_supported(block_filter::supported_models())
}

pub(crate) fn default_ir_model() -> String {
    first_supported(block_ir::supported_models())
}

pub(crate) fn default_wah_model() -> String {
    first_supported(block_wah::supported_models())
}

pub(crate) fn default_modulation_model() -> String {
    first_supported(block_mod::supported_models())
}

pub(crate) fn default_pitch_model() -> String {
    first_supported(block_pitch::supported_models())
}

pub(crate) const fn default_enabled() -> bool {
    true
}

pub(crate) fn default_instrument() -> String {
    block_core::DEFAULT_INSTRUMENT.to_string()
}

#[cfg(test)]
#[path = "default_models_tests.rs"]
mod tests;
