use crate::block::{
    build_audio_block_kind, schema_for_block_model, AudioBlockKind,
};
use crate::param::ParameterSet;
use block_core::ModelVisualData;

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

#[derive(Clone, Copy)]
struct BlockRegistryEntry {
    effect_type: &'static str,
    display_label: &'static str,
    icon_kind: &'static str,
    use_panel_editor: bool,
    supported_models: SupportedModelsFn,
    model_visual: ModelVisualFn,
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
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_AMP,
            display_label: "AMP",
            icon_kind: EFFECT_TYPE_AMP,
            use_panel_editor: true,
            supported_models: block_amp::supported_models,
            model_visual: block_amp::amp_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_CAB,
            display_label: "CAB",
            icon_kind: EFFECT_TYPE_CAB,
            use_panel_editor: true,
            supported_models: block_cab::supported_models,
            model_visual: block_cab::cab_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_BODY,
            display_label: "BODY",
            icon_kind: "body",
            use_panel_editor: true,
            supported_models: block_body::supported_models,
            model_visual: block_body::body_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_IR,
            display_label: "IR",
            icon_kind: EFFECT_TYPE_IR,
            use_panel_editor: true,
            supported_models: block_ir::supported_models,
            model_visual: block_ir::ir_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_FULL_RIG,
            display_label: "RIG",
            icon_kind: EFFECT_TYPE_FULL_RIG,
            use_panel_editor: true,
            supported_models: block_full_rig::supported_models,
            model_visual: block_full_rig::full_rig_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_GAIN,
            display_label: "GAIN",
            icon_kind: EFFECT_TYPE_GAIN,
            use_panel_editor: true,
            supported_models: block_gain::supported_models,
            model_visual: block_gain::gain_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_DYNAMICS,
            display_label: "DYN",
            icon_kind: EFFECT_TYPE_DYNAMICS,
            use_panel_editor: true,
            supported_models: block_dyn::supported_models,
            model_visual: block_dyn::dyn_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_FILTER,
            display_label: "FILTER",
            icon_kind: EFFECT_TYPE_FILTER,
            use_panel_editor: true,
            supported_models: block_filter::supported_models,
            model_visual: block_filter::filter_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_WAH,
            display_label: "WAH",
            icon_kind: EFFECT_TYPE_WAH,
            use_panel_editor: true,
            supported_models: block_wah::supported_models,
            model_visual: block_wah::wah_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_PITCH,
            display_label: "PITCH",
            icon_kind: EFFECT_TYPE_PITCH,
            use_panel_editor: true,
            supported_models: block_pitch::supported_models,
            model_visual: block_pitch::pitch_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_MODULATION,
            display_label: "MOD",
            icon_kind: EFFECT_TYPE_MODULATION,
            use_panel_editor: true,
            supported_models: block_mod::supported_models,
            model_visual: block_mod::mod_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_DELAY,
            display_label: "DLY",
            icon_kind: EFFECT_TYPE_DELAY,
            use_panel_editor: true,
            supported_models: block_delay::supported_models,
            model_visual: block_delay::delay_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_REVERB,
            display_label: "RVB",
            icon_kind: EFFECT_TYPE_REVERB,
            use_panel_editor: true,
            supported_models: block_reverb::supported_models,
            model_visual: block_reverb::reverb_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_UTILITY,
            display_label: "UTIL",
            icon_kind: EFFECT_TYPE_UTILITY,
            use_panel_editor: true,
            supported_models: block_util::supported_models,
            model_visual: block_util::util_model_visual,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_NAM,
            display_label: "NAM",
            icon_kind: EFFECT_TYPE_NAM,
            use_panel_editor: true,
            supported_models: block_nam::supported_models,
            model_visual: block_nam::nam_model_visual,
        },
    ]
}

pub fn supported_block_types() -> Vec<BlockTypeCatalogEntry> {
    let types: Vec<_> = block_registry()
        .into_iter()
        .filter(|entry| !(entry.supported_models)().is_empty())
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
            display_label: entry.display_label,
            icon_kind: entry.icon_kind,
            use_panel_editor: entry.use_panel_editor,
        })
        .collect();
    log::trace!("supported_block_types: {} types registered", types.len());
    types
}

pub fn supported_block_type(effect_type: &str) -> Option<BlockTypeCatalogEntry> {
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
                brand: visual.as_ref().map(|v| v.brand.to_string()).unwrap_or_default(),
                type_label: visual.as_ref().map(|v| v.type_label.to_string()).unwrap_or_default(),
                supported_instruments: visual.as_ref()
                    .map(|v| v.supported_instruments.iter().map(|s| s.to_string()).collect())
                    .unwrap_or_else(|| block_core::ALL_INSTRUMENTS.iter().map(|s| s.to_string()).collect()),
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

pub fn model_knob_layout(effect_type: &str, model_id: &str) -> &'static [block_core::KnobLayoutEntry] {
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

pub fn build_block_kind(
    effect_type: &str,
    model_id: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    log::debug!("building block kind: effect_type='{}', model_id='{}'", effect_type, model_id);
    build_audio_block_kind(effect_type, model_id, params)
}

#[cfg(test)]
mod tests {
    use super::{supported_block_models, supported_block_types};

    #[test]
    fn catalog_exposes_supported_types() {
        let effect_types = supported_block_types()
            .into_iter()
            .map(|entry| entry.effect_type)
            .collect::<Vec<_>>();

        assert!(effect_types.contains(&"preamp"));
        assert!(effect_types.contains(&"delay"));
        assert!(effect_types.contains(&"nam"));
        assert!(effect_types.contains(&"ir"));
        assert!(effect_types.contains(&"wah"));
        assert!(effect_types.contains(&"pitch"));
    }

    #[test]
    fn catalog_mirrors_core_supported_models() {
        let amp_model_ids = supported_block_models("preamp")
            .expect("preamp catalog")
            .into_iter()
            .map(|entry| entry.model_id)
            .collect::<Vec<_>>();
        let expected = block_preamp::supported_models()
            .iter()
            .map(|model| (*model).to_string())
            .collect::<Vec<_>>();

        assert_eq!(amp_model_ids, expected);

        let delay_model_ids = supported_block_models("delay")
            .expect("delay catalog")
            .into_iter()
            .map(|entry| entry.model_id)
            .collect::<Vec<_>>();
        let expected = block_delay::supported_models()
            .iter()
            .map(|model| (*model).to_string())
            .collect::<Vec<_>>();

        assert_eq!(delay_model_ids, expected);
    }
}
