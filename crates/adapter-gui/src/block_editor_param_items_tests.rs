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

#[test]
fn knob_row_carries_the_plugin_value_texts_as_its_labels() {
    // #1011: a VST3 knob shows the plugin's own text ("2.5 s"), not 0..100.
    let spec = project::param::float_parameter(
        "p1",
        "Decay",
        None,
        Some(0.0),
        0.0,
        100.0,
        1.0,
        project::param::ParameterUnit::Percent,
    )
    .with_value_labels(vec!["0.1 s".into(), "2.5 s".into()]);
    let row = block_parameter_item(&spec, &ParameterSet::default());
    use slint::Model;
    let labels: Vec<String> = row.option_labels.iter().map(|l| l.to_string()).collect();
    assert_eq!(labels, ["0.1 s", "2.5 s"]);
}
