//! Responsibility: answers what one catalog model is called.

use crate::catalog_label::package_type_label;
use crate::catalog_listing::block_type_for_effect_type;
use crate::catalog_registry::block_registry;

/// Returns the stream kind produced by a model's StreamHandle.
/// Empty string if the model produces no stream.
pub fn model_stream_kind(effect_type: &str, model_id: &str) -> &'static str {
    if effect_type == block_core::EFFECT_TYPE_UTILITY {
        block_util::util_stream_kind(model_id)
    } else {
        ""
    }
}

/// Look up a disk-package by effect_type + model_id, applying the same
/// `effect_type → BlockType` mapping the catalog uses for `supported_block_models`.
/// Returns `None` if the effect_type has no disk-package support or the
/// model_id isn't a disk-package id. Issue #414.
fn disk_package_for(
    effect_type: &str,
    model_id: &str,
) -> Option<&'static plugin_loader::discover::LoadedPackage> {
    let block_type = block_type_for_effect_type(effect_type)?;
    plugin_loader::registry::packages_for(block_type)
        .into_iter()
        .find(|p| p.manifest.id == model_id)
}

/// Returns the display name for a model, or empty string if not found.
///
/// Native models resolve via the per-effect `block_*::display_name`; disk-package
/// models (NAM/IR/LV2/VST3) fall back to the plugin_loader registry so the
/// hover tooltip, plugin-info window and block editor header all show the right
/// name. Issue #414.
pub fn model_display_name(effect_type: &str, model_id: &str) -> String {
    use block_core::*;
    let native: &'static str = match effect_type {
        EFFECT_TYPE_UTILITY => block_util::util_display_name(model_id),
        EFFECT_TYPE_GAIN => block_gain::gain_display_name(model_id),
        EFFECT_TYPE_AMP => block_amp::amp_display_name(model_id),
        EFFECT_TYPE_PREAMP => block_preamp::preamp_display_name(model_id).unwrap_or(""),
        EFFECT_TYPE_CAB => block_cab::cab_display_name(model_id),
        EFFECT_TYPE_DELAY => block_delay::delay_display_name(model_id),
        EFFECT_TYPE_REVERB => block_reverb::reverb_display_name(model_id),
        EFFECT_TYPE_MODULATION => block_mod::mod_display_name(model_id),
        EFFECT_TYPE_DYNAMICS => block_dyn::dyn_display_name(model_id),
        EFFECT_TYPE_FILTER => block_filter::filter_display_name(model_id),
        EFFECT_TYPE_WAH => block_wah::wah_display_name(model_id),
        EFFECT_TYPE_PITCH => block_pitch::pitch_display_name(model_id),
        EFFECT_TYPE_BODY => block_body::body_display_name(model_id),
        EFFECT_TYPE_FULL_RIG => block_full_rig::full_rig_display_name(model_id),
        EFFECT_TYPE_NAM => block_nam::nam_display_name(model_id),
        EFFECT_TYPE_IR => block_ir::ir_display_name(model_id),
        _ => "",
    };
    if !native.is_empty() {
        return native.to_string();
    }
    disk_package_for(effect_type, model_id)
        .map(|p| p.manifest.display_name.clone())
        .unwrap_or_default()
}

/// Returns the brand for a model, or empty string if not found.
///
/// Native first, then disk-package `manifest.brand`. Issue #414.
pub fn model_brand(effect_type: &str, model_id: &str) -> String {
    use block_core::*;
    let native: &'static str = match effect_type {
        EFFECT_TYPE_UTILITY => block_util::util_brand(model_id),
        EFFECT_TYPE_GAIN => block_gain::gain_brand(model_id),
        EFFECT_TYPE_AMP => block_amp::amp_model_visual(model_id)
            .map(|v| v.brand)
            .unwrap_or(""),
        EFFECT_TYPE_PREAMP => block_preamp::preamp_brand(model_id).unwrap_or(""),
        EFFECT_TYPE_CAB => block_cab::cab_brand(model_id),
        EFFECT_TYPE_DELAY => block_delay::delay_brand(model_id),
        EFFECT_TYPE_REVERB => block_reverb::reverb_brand(model_id),
        EFFECT_TYPE_MODULATION => block_mod::mod_brand(model_id),
        EFFECT_TYPE_DYNAMICS => block_dyn::dyn_brand(model_id),
        EFFECT_TYPE_FILTER => block_filter::filter_brand(model_id),
        EFFECT_TYPE_WAH => block_wah::wah_brand(model_id),
        EFFECT_TYPE_PITCH => block_pitch::pitch_brand(model_id),
        EFFECT_TYPE_BODY => block_body::body_brand(model_id),
        EFFECT_TYPE_FULL_RIG => block_full_rig::full_rig_brand(model_id),
        EFFECT_TYPE_NAM => block_nam::nam_brand(model_id),
        EFFECT_TYPE_IR => block_ir::ir_brand(model_id),
        _ => "",
    };
    if !native.is_empty() {
        return native.to_string();
    }
    disk_package_for(effect_type, model_id)
        .and_then(|p| p.manifest.brand.clone())
        .unwrap_or_default()
}

/// Returns the type label for a model (e.g. "NATIVE", "NAM", "LV2", "IR"),
/// or empty string if not found.
///
/// Native first, then [`package_type_label`] on the disk-package — so a NAM
/// package shows the same NAM/A1 vs NAM/A2 badge here (tooltip, plugin-info,
/// block-editor header) as in the picker. Issues #414, #650.
pub fn model_type_label(effect_type: &str, model_id: &str) -> String {
    use block_core::*;
    let native: &'static str = match effect_type {
        EFFECT_TYPE_UTILITY => block_util::util_type_label(model_id),
        EFFECT_TYPE_GAIN => block_gain::gain_type_label(model_id),
        EFFECT_TYPE_AMP => block_amp::amp_model_visual(model_id)
            .map(|v| v.type_label)
            .unwrap_or(""),
        EFFECT_TYPE_PREAMP => block_preamp::preamp_type_label(model_id).unwrap_or(""),
        EFFECT_TYPE_CAB => block_cab::cab_type_label(model_id),
        EFFECT_TYPE_DELAY => block_delay::delay_type_label(model_id),
        EFFECT_TYPE_REVERB => block_reverb::reverb_type_label(model_id),
        EFFECT_TYPE_MODULATION => block_mod::mod_type_label(model_id),
        EFFECT_TYPE_DYNAMICS => block_dyn::dyn_type_label(model_id),
        EFFECT_TYPE_FILTER => block_filter::filter_type_label(model_id),
        EFFECT_TYPE_WAH => block_wah::wah_type_label(model_id),
        EFFECT_TYPE_PITCH => block_pitch::pitch_type_label(model_id),
        EFFECT_TYPE_BODY => block_body::body_type_label(model_id),
        EFFECT_TYPE_FULL_RIG => block_full_rig::full_rig_type_label(model_id),
        EFFECT_TYPE_NAM => block_nam::nam_type_label(model_id),
        EFFECT_TYPE_IR => block_ir::ir_type_label(model_id),
        _ => "",
    };
    if !native.is_empty() {
        return native.to_string();
    }
    disk_package_for(effect_type, model_id)
        .map(|p| package_type_label(&p.manifest))
        .unwrap_or_default()
}

pub fn model_knob_layout(
    effect_type: &str,
    model_id: &str,
) -> &'static [block_core::KnobLayoutEntry] {
    let entry = block_registry()
        .into_iter()
        .find(|entry| entry.effect_type == effect_type);
    match entry {
        Some(e) => (e.model_visual)(model_id)
            .map(|v| v.knob_layout)
            .unwrap_or(&[]),
        None => &[],
    }
}

/// True when a block opens its own native editor (compact view then shows an
/// "open editor" action instead of inline knobs). None do since #780 removed the
/// native VST3 editor — VST3 params are OpenRig knobs now (was hiding its strip).
pub fn block_has_external_gui(_effect_type: &str) -> bool {
    false
}
