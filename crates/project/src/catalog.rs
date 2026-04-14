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
        EFFECT_TYPE_AMP => block_amp::amp_model_visual(model_id).map(|v| v.brand).unwrap_or(""),
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
        EFFECT_TYPE_AMP => block_amp::amp_model_visual(model_id).map(|v| v.type_label).unwrap_or(""),
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

/// Returns true when a block opens its own native editor window,
/// meaning the UI should show an "open editor" action instead
/// of rendering inline parameter controls.
pub fn block_has_external_gui(effect_type: &str, model_id: &str) -> bool {
    model_type_label(effect_type, model_id) == "VST3"
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

    // --- model_display_name tests ---

    #[test]
    fn model_display_name_known_preamp_returns_nonempty() {
        let models = block_preamp::supported_models();
        let name = super::model_display_name("preamp", models[0]);
        assert!(!name.is_empty(), "display_name for known preamp should be non-empty");
    }

    #[test]
    fn model_display_name_unknown_type_returns_empty() {
        let name = super::model_display_name("nonexistent", "some_model");
        assert_eq!(name, "");
    }

    #[test]
    fn model_display_name_unknown_model_returns_empty() {
        let name = super::model_display_name("preamp", "nonexistent_model_xyz");
        assert_eq!(name, "");
    }

    #[test]
    fn model_display_name_all_effect_types_known_model() {
        let type_model_pairs: Vec<(&str, &str)> = vec![
            ("delay", block_delay::supported_models()[0]),
            ("reverb", block_reverb::supported_models()[0]),
            ("gain", block_gain::supported_models()[0]),
            ("dynamics", block_dyn::supported_models()[0]),
            ("filter", block_filter::supported_models()[0]),
            ("wah", block_wah::supported_models()[0]),
            ("pitch", block_pitch::supported_models()[0]),
            ("modulation", block_mod::supported_models()[0]),
            ("utility", block_util::supported_models()[0]),
            ("amp", block_amp::supported_models()[0]),
            ("cab", block_cab::supported_models()[0]),
            ("body", block_body::supported_models()[0]),
            ("ir", block_ir::supported_models()[0]),
            ("nam", block_nam::supported_models()[0]),
        ];
        for (effect_type, model_id) in type_model_pairs {
            let name = super::model_display_name(effect_type, model_id);
            assert!(
                !name.is_empty(),
                "display_name for {effect_type}:{model_id} should be non-empty"
            );
        }
    }

    // --- model_brand tests ---

    #[test]
    fn model_brand_known_preamp_returns_string() {
        let models = block_preamp::supported_models();
        let brand = super::model_brand("preamp", models[0]);
        // brand can be empty for some models, but shouldn't panic
        let _ = brand;
    }

    #[test]
    fn model_brand_unknown_type_returns_empty() {
        let brand = super::model_brand("nonexistent", "some_model");
        assert_eq!(brand, "");
    }

    #[test]
    fn model_brand_all_effect_types() {
        let type_model_pairs: Vec<(&str, &str)> = vec![
            ("delay", block_delay::supported_models()[0]),
            ("reverb", block_reverb::supported_models()[0]),
            ("gain", block_gain::supported_models()[0]),
            ("dynamics", block_dyn::supported_models()[0]),
            ("filter", block_filter::supported_models()[0]),
            ("wah", block_wah::supported_models()[0]),
            ("pitch", block_pitch::supported_models()[0]),
            ("modulation", block_mod::supported_models()[0]),
            ("utility", block_util::supported_models()[0]),
            ("amp", block_amp::supported_models()[0]),
            ("cab", block_cab::supported_models()[0]),
            ("body", block_body::supported_models()[0]),
            ("ir", block_ir::supported_models()[0]),
            ("nam", block_nam::supported_models()[0]),
        ];
        for (effect_type, model_id) in type_model_pairs {
            // Should not panic for any known effect type
            let _ = super::model_brand(effect_type, model_id);
        }
    }

    // --- model_type_label tests ---

    #[test]
    fn model_type_label_known_preamp_returns_nonempty() {
        let models = block_preamp::supported_models();
        let label = super::model_type_label("preamp", models[0]);
        assert!(!label.is_empty(), "type_label for known preamp should be non-empty");
    }

    #[test]
    fn model_type_label_unknown_type_returns_empty() {
        let label = super::model_type_label("nonexistent", "some_model");
        assert_eq!(label, "");
    }

    #[test]
    fn model_type_label_all_effect_types() {
        let type_model_pairs: Vec<(&str, &str)> = vec![
            ("delay", block_delay::supported_models()[0]),
            ("reverb", block_reverb::supported_models()[0]),
            ("gain", block_gain::supported_models()[0]),
            ("dynamics", block_dyn::supported_models()[0]),
            ("filter", block_filter::supported_models()[0]),
            ("wah", block_wah::supported_models()[0]),
            ("pitch", block_pitch::supported_models()[0]),
            ("modulation", block_mod::supported_models()[0]),
            ("utility", block_util::supported_models()[0]),
            ("amp", block_amp::supported_models()[0]),
            ("cab", block_cab::supported_models()[0]),
            ("body", block_body::supported_models()[0]),
            ("ir", block_ir::supported_models()[0]),
            ("nam", block_nam::supported_models()[0]),
        ];
        for (effect_type, model_id) in type_model_pairs {
            let label = super::model_type_label(effect_type, model_id);
            assert!(
                !label.is_empty(),
                "type_label for {effect_type}:{model_id} should be non-empty"
            );
        }
    }

    // --- block_has_external_gui tests ---

    #[test]
    fn block_has_external_gui_vst3_reverb_returns_true() {
        assert!(super::block_has_external_gui("reverb", "vst3_cloud_seed"));
    }

    #[test]
    fn block_has_external_gui_native_reverb_returns_false() {
        assert!(!super::block_has_external_gui("reverb", "hall"));
    }

    #[test]
    fn block_has_external_gui_native_preamp_returns_false() {
        assert!(!super::block_has_external_gui("preamp", "american_clean"));
    }

    // --- supported_block_models for all effect types ---

    #[test]
    fn supported_block_models_all_registered_types() {
        let registered_types = supported_block_types()
            .into_iter()
            .map(|entry| entry.effect_type)
            .collect::<Vec<_>>();

        for effect_type in registered_types {
            if effect_type == "vst3" {
                continue; // VST3 depends on runtime discovery
            }
            let models = supported_block_models(effect_type)
                .unwrap_or_else(|e| panic!("supported_block_models({effect_type}) failed: {e}"));
            assert!(
                !models.is_empty(),
                "{effect_type} should have at least one model"
            );
            for model in &models {
                assert!(!model.model_id.is_empty());
                assert!(!model.display_name.is_empty());
                assert_eq!(model.effect_type, effect_type);
            }
        }
    }

    #[test]
    fn supported_block_models_unsupported_type_errors() {
        let result = supported_block_models("nonexistent_type");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported effect type"));
    }

    // --- supported_block_type tests ---

    #[test]
    fn supported_block_type_known_type_returns_some() {
        let entry = super::supported_block_type("preamp");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.effect_type, "preamp");
        assert_eq!(entry.display_label, "PREAMP");
    }

    #[test]
    fn supported_block_type_vst3_returns_some() {
        let entry = super::supported_block_type("vst3");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().display_label, "VST3");
    }

    #[test]
    fn supported_block_type_unknown_returns_none() {
        assert!(super::supported_block_type("nonexistent").is_none());
    }

    // --- model_stream_kind tests ---

    #[test]
    fn model_stream_kind_non_utility_returns_empty() {
        assert_eq!(super::model_stream_kind("delay", "some_model"), "");
        assert_eq!(super::model_stream_kind("preamp", "american_clean"), "");
    }

    #[test]
    fn model_stream_kind_utility_returns_value() {
        let model = block_util::supported_models()[0];
        // Should not panic; may return empty or a stream kind string
        let _ = super::model_stream_kind("utility", model);
    }

    // --- model_knob_layout tests ---

    #[test]
    fn model_knob_layout_unknown_type_returns_empty() {
        let layout = super::model_knob_layout("nonexistent", "model");
        assert!(layout.is_empty());
    }

    #[test]
    fn model_knob_layout_known_type_returns_slice() {
        let model = block_delay::supported_models()[0];
        // Should not panic; may return empty or populated slice
        let _ = super::model_knob_layout("delay", model);
    }

    // --- build_block_kind tests ---

    #[test]
    fn build_block_kind_valid_model_succeeds() {
        let model = block_reverb::supported_models()[0];
        let schema = crate::block::schema_for_block_model("reverb", model).unwrap();
        let params = crate::param::ParameterSet::default()
            .normalized_against(&schema)
            .unwrap();
        let kind = super::build_block_kind("reverb", model, params);
        assert!(kind.is_ok());
    }

    #[test]
    fn build_block_kind_invalid_type_errors() {
        let result = super::build_block_kind("nonexistent", "model", crate::param::ParameterSet::default());
        assert!(result.is_err());
    }

    // --- catalog model entries have supported_instruments ---

    #[test]
    fn catalog_model_entries_have_supported_instruments() {
        let models = supported_block_models("preamp").unwrap();
        for model in &models {
            assert!(
                !model.supported_instruments.is_empty(),
                "preamp model {} should have supported_instruments",
                model.model_id
            );
        }
    }
}
