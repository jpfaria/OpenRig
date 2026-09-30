use super::*;

fn item(label: &str, group: &str) -> BlockParameterItem {
    BlockParameterItem {
        label: label.into(),
        group: group.into(),
        ..Default::default()
    }
}

#[test]
fn groups_are_distinct_first_appearance_with_default_fallback() {
    let items = vec![
        item("Gain", "Tone"),
        item("Level", "Tone"),
        item("Mode", "Voicing"),
        item("Mix", ""), // ungrouped → Main
    ];
    assert_eq!(
        parameter_groups(&items),
        vec![
            "Tone".to_string(),
            "Voicing".to_string(),
            "Main".to_string()
        ]
    );
}

#[test]
fn vst3_block_yields_params_via_the_compact_build_path() {
    // #780 repro: the compact view showed a VST3 block with zero params.
    // This drives the SAME build the compact view uses — build the block,
    // resolve its editor data, build the param items — with a real plugin.
    use std::path::PathBuf;
    let Some(dir) = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from) else {
        return;
    };
    project::vst3_editor::init_vst3_catalog(48_000.0, &[dir]);
    let Some(model) = project::catalog::supported_block_models(block_core::EFFECT_TYPE_VST3)
        .ok()
        .and_then(|models| {
            models
                .into_iter()
                .find(|m| m.model_id.to_lowercase().contains("chowcentaur"))
                .map(|m| m.model_id)
        })
    else {
        return;
    };
    let kind = project::block::build_audio_block_kind(
        block_core::EFFECT_TYPE_VST3,
        &model,
        project::param::ParameterSet::default(),
    )
    .expect("build vst3 block kind");
    let block = project::block::AudioBlock {
        id: domain::ids::BlockId("v1".into()),
        enabled: true,
        kind,
    };
    let data = crate::block_editor::block_editor_data(&block).expect("editor data for vst3 block");
    let params = block_parameter_items_for_editor(&data);
    assert!(
        !params.is_empty(),
        "compact build path produced ZERO params for VST3 model {}",
        model
    );
}

/// #328: the split editor feeds its own specs (no catalog model) to the same
/// row builder the block editor uses.
#[test]
fn rows_build_from_a_bare_spec_list() {
    use project::param::{bool_parameter, float_parameter, ParameterSet, ParameterUnit};
    let specs = vec![
        bool_parameter("sum", "Sum", None, Some(false)),
        float_parameter(
            "level",
            "Level",
            None,
            Some(100.0),
            0.0,
            100.0,
            1.0,
            ParameterUnit::Percent,
        ),
    ];
    let rows = super::block_parameter_items_for_specs(&specs, &ParameterSet::default());
    let shape: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.path.to_string(), r.widget_kind.to_string()))
        .collect();
    assert_eq!(shape[0], ("sum".to_string(), "bool".to_string()));
    assert_eq!(shape[1].0, "level");
    assert_eq!(
        rows[1].numeric_value, 100.0,
        "the spec default fills an unset value"
    );
}
