use crate::block::{build_audio_block_kind, schema_for_block_model, AudioBlockKind};
use crate::param::ParameterSet;
use block_core::{ModelColorOverride, ModelColorScheme, ModelVisualData};

/// Used in the BlockRegistryEntry rows for block crates that have no
/// native model overrides (block-body, block-full-rig, block-gain,
/// block-ir, block-nam, block-pitch, block-util). Returning `None`
/// here makes the resolution fall through to the brand colors only.
fn no_color_override(_: &str) -> Option<ModelColorOverride> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTypeCatalogEntry {
    pub effect_type: &'static str,
    pub display_label: &'static str,
    pub icon_kind: &'static str,
    pub use_panel_editor: bool,
}

#[derive(Debug, Clone)]
pub struct BlockModelCatalogEntry {
    pub effect_type: String,
    pub model_id: String,
    pub display_name: String,
    pub brand: String,
    pub type_label: String,
    pub supported_instruments: Vec<String>,
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
}

type SupportedModelsFn = fn() -> &'static [&'static str];
type ModelVisualFn = fn(&str) -> Option<ModelVisualData>;
type ModelColorOverrideFn = fn(&str) -> Option<ModelColorOverride>;

#[derive(Clone, Copy)]
struct BlockRegistryEntry {
    effect_type: &'static str,
    display_label: &'static str,
    icon_kind: &'static str,
    use_panel_editor: bool,
    supported_models: SupportedModelsFn,
    model_visual: ModelVisualFn,
    model_color_override: ModelColorOverrideFn,
}

fn block_registry() -> [BlockRegistryEntry; 16] {
    use block_core::*;
    [
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_PREAMP,
            display_label: "PREAMP",
            icon_kind: EFFECT_TYPE_PREAMP,
            use_panel_editor: true,
            supported_models: block_preamp::supported_models,
            model_visual: block_preamp::preamp_model_visual,
            model_color_override: block_preamp::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_AMP,
            display_label: "AMP",
            icon_kind: EFFECT_TYPE_AMP,
            use_panel_editor: true,
            supported_models: block_amp::supported_models,
            model_visual: block_amp::amp_model_visual,
            model_color_override: block_amp::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_CAB,
            display_label: "CAB",
            icon_kind: EFFECT_TYPE_CAB,
            use_panel_editor: true,
            supported_models: block_cab::supported_models,
            model_visual: block_cab::cab_model_visual,
            model_color_override: block_cab::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_BODY,
            display_label: "BODY",
            icon_kind: "body",
            use_panel_editor: true,
            supported_models: block_body::supported_models,
            model_visual: block_body::body_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_IR,
            display_label: "IR",
            icon_kind: EFFECT_TYPE_IR,
            use_panel_editor: true,
            supported_models: block_ir::supported_models,
            model_visual: block_ir::ir_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_FULL_RIG,
            display_label: "RIG",
            icon_kind: EFFECT_TYPE_FULL_RIG,
            use_panel_editor: true,
            supported_models: block_full_rig::supported_models,
            model_visual: block_full_rig::full_rig_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_GAIN,
            display_label: "GAIN",
            icon_kind: EFFECT_TYPE_GAIN,
            use_panel_editor: true,
            supported_models: block_gain::supported_models,
            model_visual: block_gain::gain_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_DYNAMICS,
            display_label: "DYN",
            icon_kind: EFFECT_TYPE_DYNAMICS,
            use_panel_editor: true,
            supported_models: block_dyn::supported_models,
            model_visual: block_dyn::dyn_model_visual,
            model_color_override: block_dyn::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_FILTER,
            display_label: "FILTER",
            icon_kind: EFFECT_TYPE_FILTER,
            use_panel_editor: true,
            supported_models: block_filter::supported_models,
            model_visual: block_filter::filter_model_visual,
            model_color_override: block_filter::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_WAH,
            display_label: "WAH",
            icon_kind: EFFECT_TYPE_WAH,
            use_panel_editor: true,
            supported_models: block_wah::supported_models,
            model_visual: block_wah::wah_model_visual,
            model_color_override: block_wah::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_PITCH,
            display_label: "PITCH",
            icon_kind: EFFECT_TYPE_PITCH,
            use_panel_editor: true,
            supported_models: block_pitch::supported_models,
            model_visual: block_pitch::pitch_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_MODULATION,
            display_label: "MOD",
            icon_kind: EFFECT_TYPE_MODULATION,
            use_panel_editor: true,
            supported_models: block_mod::supported_models,
            model_visual: block_mod::mod_model_visual,
            model_color_override: block_mod::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_DELAY,
            display_label: "DLY",
            icon_kind: EFFECT_TYPE_DELAY,
            use_panel_editor: true,
            supported_models: block_delay::supported_models,
            model_visual: block_delay::delay_model_visual,
            model_color_override: block_delay::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_REVERB,
            display_label: "RVB",
            icon_kind: EFFECT_TYPE_REVERB,
            use_panel_editor: true,
            supported_models: block_reverb::supported_models,
            model_visual: block_reverb::reverb_model_visual,
            model_color_override: block_reverb::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_UTILITY,
            display_label: "UTIL",
            icon_kind: EFFECT_TYPE_UTILITY,
            use_panel_editor: true,
            supported_models: block_util::supported_models,
            model_visual: block_util::util_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_NAM,
            display_label: "NAM",
            icon_kind: EFFECT_TYPE_NAM,
            use_panel_editor: true,
            supported_models: block_nam::supported_models,
            model_visual: block_nam::nam_model_visual,
            model_color_override: no_color_override,
        },
    ]
}

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

pub fn supported_block_types() -> Vec<BlockTypeCatalogEntry> {
    let mut types: Vec<_> = block_registry()
        .into_iter()
        .filter(|entry| !(entry.supported_models)().is_empty())
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
            display_label: entry.display_label,
            icon_kind: entry.icon_kind,
            use_panel_editor: entry.use_panel_editor,
        })
        .collect();
    // Include the VST3 dynamic type only if plugins have been discovered.
    if !vst3_host::vst3_catalog().is_empty() {
        types.push(BlockTypeCatalogEntry {
            effect_type: block_core::EFFECT_TYPE_VST3,
            display_label: "VST3",
            icon_kind: block_core::EFFECT_TYPE_VST3,
            use_panel_editor: true,
        });
    }
    log::trace!("supported_block_types: {} types registered", types.len());
    types
}

pub fn supported_block_type(effect_type: &str) -> Option<BlockTypeCatalogEntry> {
    if effect_type == block_core::EFFECT_TYPE_VST3 {
        return Some(BlockTypeCatalogEntry {
            effect_type: block_core::EFFECT_TYPE_VST3,
            display_label: "VST3",
            icon_kind: block_core::EFFECT_TYPE_VST3,
            use_panel_editor: true,
        });
    }
    block_registry()
        .into_iter()
        .find(|entry| entry.effect_type == effect_type)
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
            display_label: entry.display_label,
            icon_kind: entry.icon_kind,
            use_panel_editor: entry.use_panel_editor,
        })
}

pub fn supported_block_models(effect_type: &str) -> Result<Vec<BlockModelCatalogEntry>, String> {
    log::trace!("looking up models for effect_type='{}'", effect_type);

    // Dynamic VST3 catalog — bypass the static block_registry.
    if effect_type == block_core::EFFECT_TYPE_VST3 {
        return Ok(vst3_host::vst3_catalog()
            .iter()
            .map(|entry| BlockModelCatalogEntry {
                effect_type: block_core::EFFECT_TYPE_VST3.to_string(),
                model_id: entry.model_id.to_string(),
                display_name: entry.display_name.to_string(),
                brand: entry.brand.to_string(),
                type_label: "VST3".to_string(),
                supported_instruments: block_core::ALL_INSTRUMENTS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                knob_layout: &[],
            })
            .collect());
    }

    let entry = block_registry()
        .into_iter()
        .find(|entry| entry.effect_type == effect_type)
        .ok_or_else(|| format!("unsupported effect type '{}'", effect_type))?;

    (entry.supported_models)()
        .iter()
        .map(|model_id| {
            let schema = schema_for_block_model(effect_type, model_id)?;
            let visual = (entry.model_visual)(model_id);
            Ok(BlockModelCatalogEntry {
                effect_type: effect_type.to_string(),
                model_id: (*model_id).to_string(),
                display_name: schema.display_name,
                brand: visual
                    .as_ref()
                    .map(|v| v.brand.to_string())
                    .unwrap_or_default(),
                type_label: visual
                    .as_ref()
                    .map(|v| v.type_label.to_string())
                    .unwrap_or_default(),
                supported_instruments: visual
                    .as_ref()
                    .map(|v| {
                        v.supported_instruments
                            .iter()
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        block_core::ALL_INSTRUMENTS
                            .iter()
                            .map(|s| s.to_string())
                            .collect()
                    }),
                knob_layout: visual.as_ref().map(|v| v.knob_layout).unwrap_or(&[]),
            })
        })
        .collect()
}

/// Returns the stream kind produced by a model's StreamHandle.
/// Empty string if the model produces no stream.
pub fn model_stream_kind(effect_type: &str, model_id: &str) -> &'static str {
    if effect_type == block_core::EFFECT_TYPE_UTILITY {
        block_util::util_stream_kind(model_id)
    } else {
        ""
    }
}

/// Returns the display name for a model, or empty string if not found.
pub fn model_display_name(effect_type: &str, model_id: &str) -> &'static str {
    use block_core::*;
    match effect_type {
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
    }
}

/// Returns the brand for a model, or empty string if not found.
pub fn model_brand(effect_type: &str, model_id: &str) -> &'static str {
    use block_core::*;
    match effect_type {
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
    }
}

/// Returns the type label for a model (e.g. "NATIVE", "NAM", "LV2", "IR"),
/// or empty string if not found.
pub fn model_type_label(effect_type: &str, model_id: &str) -> &'static str {
    use block_core::*;
    match effect_type {
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
    }
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

/// Returns true when a block type opens its own native editor window,
/// meaning the compact view should show an "open editor" action instead
/// of rendering inline parameter controls.
pub fn block_has_external_gui(effect_type: &str) -> bool {
    effect_type == block_core::EFFECT_TYPE_VST3
}

pub fn build_block_kind(
    effect_type: &str,
    model_id: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    log::debug!(
        "building block kind: effect_type='{}', model_id='{}'",
        effect_type,
        model_id
    );
    build_audio_block_kind(effect_type, model_id, params)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

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

/// Returns the catalog thumbnail path (relative to project root) for an LV2 model.
pub fn model_thumbnail(effect_type: &str, model_id: &str) -> Option<&'static str> {
    use block_core::*;
    match effect_type {
        EFFECT_TYPE_REVERB => block_reverb::reverb_thumbnail(model_id),
        EFFECT_TYPE_DELAY => block_delay::delay_thumbnail(model_id),
        EFFECT_TYPE_MODULATION => block_mod::mod_thumbnail(model_id),
        EFFECT_TYPE_FILTER => block_filter::filter_thumbnail(model_id),
        EFFECT_TYPE_DYNAMICS => block_dyn::dyn_thumbnail(model_id),
        EFFECT_TYPE_GAIN => block_gain::gain_thumbnail(model_id),
        EFFECT_TYPE_PITCH => block_pitch::pitch_thumbnail(model_id),
        _ => None,
    }
}
