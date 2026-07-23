//! Unit tests for crate-root helpers (CLI parsing, project session bootstrap,
//! select-block editor data shaping, GUI ↔ project device-setting conversion).
//!
//! Lifted out of `lib.rs` so the crate root stays a thin orchestrator. All
//! tests live under `#[cfg(test)]` and only run with `cargo test`.

use super::{open_cli_project, parse_cli_args_from, SELECT_SELECTED_BLOCK_ID};
use crate::block_editor::{block_editor_data, block_parameter_items_for_editor};
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SelectBlock};
use project::catalog::supported_block_models;
use project::param::ParameterSet;
use slint::Model;
#[test]
fn select_block_editor_uses_selected_option_model() {
    let delay_models = delay_model_ids();
    let first_model = delay_models
        .first()
        .expect("delay catalog must not be empty");
    let second_model = delay_models.get(1).unwrap_or(first_model);
    let block = select_delay_block(
        "chain:0:block:0",
        first_model.as_str(),
        second_model.as_str(),
    );
    let editor_data = block_editor_data(&block).expect("select should expose editor data");
    assert!(editor_data.is_select);
    assert_eq!(editor_data.effect_type, "delay");
    assert_eq!(editor_data.model_id, second_model.as_str());
    assert_eq!(editor_data.select_options.len(), 2);
    assert_eq!(
        editor_data.selected_select_option_block_id.as_deref(),
        Some("chain:0:block:0::delay_b")
    );
}
#[test]
fn select_block_editor_includes_active_option_picker() {
    let delay_models = delay_model_ids();
    let first_model = delay_models
        .first()
        .expect("delay catalog must not be empty");
    let second_model = delay_models.get(1).unwrap_or(first_model);
    let block = select_delay_block(
        "chain:0:block:0",
        first_model.as_str(),
        second_model.as_str(),
    );
    let editor_data = block_editor_data(&block).expect("select should expose editor data");
    let items = block_parameter_items_for_editor(&editor_data);
    let selector = items
        .iter()
        .find(|item| item.path.as_str() == SELECT_SELECTED_BLOCK_ID)
        .expect("select editor should expose active option picker");
    assert_eq!(selector.option_values.row_count(), 2);
    assert_eq!(selector.selected_option_index, 1);
}
fn select_delay_block(id: &str, first_model: &str, second_model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId(format!("{id}::delay_b")),
            options: vec![
                delay_block(format!("{id}::delay_a"), first_model, 120.0),
                delay_block(format!("{id}::delay_b"), second_model, 240.0),
            ],
        }),
    }
}
fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
    let schema = schema_for_block_model("delay", model).expect("delay schema should exist");
    let mut params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults should normalize");
    params.insert("time_ms", ParameterValue::Float(time_ms));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}
pub(super) fn delay_model_ids() -> Vec<String> {
    supported_block_models("delay")
        .expect("delay catalog should exist")
        .into_iter()
        .map(|entry| entry.model_id)
        .collect()
}

// --- ui_index_to_real_block_index tests ---

use crate::runtime_lifecycle::ui_index_to_real_block_index;
use domain::ids::ChainId;
use project::block::{InputBlock, OutputBlock};
use project::chain::{Chain, ChainInputMode};

pub(super) fn test_chain(block_kinds: Vec<AudioBlockKind>) -> Chain {
    Chain {
        id: ChainId("test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: block_kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| AudioBlock {
                id: BlockId(format!("block:{}", i)),
                enabled: true,
                kind,
            })
            .collect(),
        di_output: None,
    }
}

pub(super) fn input_kind() -> AudioBlockKind {
    AudioBlockKind::Input(InputBlock {
        model: "standard".into(),
        io: String::new(),
        endpoint: String::new(),
    })
}

pub(super) fn output_kind() -> AudioBlockKind {
    AudioBlockKind::Output(OutputBlock {
        model: "standard".into(),
        io: String::new(),
        endpoint: String::new(),
    })
}

pub(super) fn effect_kind(effect_type: &str) -> AudioBlockKind {
    AudioBlockKind::Core(CoreBlock {
        effect_type: effect_type.into(),
        model: "test".into(),
        params: ParameterSet::default(),
    })
}

#[test]
fn ui_index_maps_correctly_with_standard_chain() {
    // [Input, Comp, Preamp, Delay, Output]
    // UI sees: [Comp(0), Preamp(1), Delay(2)]
    // Real:    [0=Input, 1=Comp, 2=Preamp, 3=Delay, 4=Output]
    let chain = test_chain(vec![
        input_kind(),
        effect_kind("dynamics"),
        effect_kind("preamp"),
        effect_kind("delay"),
        output_kind(),
    ]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 1); // UI 0 = Comp = real 1
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 2); // UI 1 = Preamp = real 2
    assert_eq!(ui_index_to_real_block_index(&chain, 2), 3); // UI 2 = Delay = real 3
}

#[test]
fn ui_index_past_end_returns_before_last_output() {
    let chain = test_chain(vec![input_kind(), effect_kind("delay"), output_kind()]);
    // UI sees [Delay(0)], asking for UI index 1 (past end) → before Output = real 2
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 2);
}

#[test]
fn ui_index_with_extra_input_in_middle() {
    // [Input, Comp, Input2, Delay, Output]
    // Hidden: first Input (0) and last Output (4)
    // UI sees: [Comp(0), Input2(1), Delay(2)]
    // Real:    [0=Input, 1=Comp, 2=Input2, 3=Delay, 4=Output]
    let chain = test_chain(vec![
        input_kind(),
        effect_kind("dynamics"),
        input_kind(),
        effect_kind("delay"),
        output_kind(),
    ]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 1); // Comp
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 2); // Input2
    assert_eq!(ui_index_to_real_block_index(&chain, 2), 3); // Delay
}

#[test]
fn ui_index_with_extra_output_in_middle() {
    // [Input, Comp, Output_mid, Delay, Output]
    // Hidden: first Input (0) and last Output (4)
    // UI sees: [Comp(0), Output_mid(1), Delay(2)]
    let chain = test_chain(vec![
        input_kind(),
        effect_kind("dynamics"),
        output_kind(),
        effect_kind("delay"),
        output_kind(),
    ]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 1); // Comp
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 2); // Output_mid (visible!)
    assert_eq!(ui_index_to_real_block_index(&chain, 2), 3); // Delay
}

#[test]
fn ui_index_with_no_io_blocks() {
    // [Comp, Delay] — no I/O blocks at all
    let chain = test_chain(vec![effect_kind("dynamics"), effect_kind("delay")]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 0);
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 1);
}

#[test]
fn ui_index_with_only_io_blocks() {
    // [Input, Output] — no effect blocks
    let chain = test_chain(vec![input_kind(), output_kind()]);
    // UI sees nothing, asking for 0 → before Output = real 1
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 1);
}

// --- format_channel_list ---

use super::project_view::format_channel_list;

#[test]
fn format_channel_list_empty_returns_dash() {
    assert_eq!(format_channel_list(&[]), "-");
}

#[test]
fn format_channel_list_single_channel_is_one_indexed() {
    assert_eq!(format_channel_list(&[0]), "1");
    assert_eq!(format_channel_list(&[3]), "4");
}

#[test]
fn format_channel_list_multiple_channels_comma_separated() {
    assert_eq!(format_channel_list(&[0, 1]), "1, 2");
    assert_eq!(format_channel_list(&[2, 5, 7]), "3, 6, 8");
}

// --- unit_label ---

use super::block_editor_values::unit_label;
use project::param::ParameterUnit;

#[test]
fn unit_label_returns_correct_suffix_for_all_variants() {
    assert_eq!(unit_label(&ParameterUnit::None), "");
    assert_eq!(unit_label(&ParameterUnit::Decibels), "dB");
    assert_eq!(unit_label(&ParameterUnit::Hertz), "Hz");
    assert_eq!(unit_label(&ParameterUnit::Milliseconds), "ms");
    assert_eq!(unit_label(&ParameterUnit::Percent), "%");
    assert_eq!(unit_label(&ParameterUnit::Ratio), "Ratio");
    assert_eq!(unit_label(&ParameterUnit::Semitones), "st");
}

// --- insert_mode_to_index / insert_mode_from_index ---

use crate::chain_editor::insert_mode_from_index;
use crate::chain_editor::insert_mode_to_index;

#[test]
fn insert_mode_mono_roundtrip() {
    assert_eq!(insert_mode_to_index(ChainInputMode::Mono), 0);
    assert_eq!(insert_mode_from_index(0), ChainInputMode::Mono);
}

#[test]
fn insert_mode_stereo_roundtrip() {
    assert_eq!(insert_mode_to_index(ChainInputMode::Stereo), 1);
    assert_eq!(insert_mode_from_index(1), ChainInputMode::Stereo);
}

#[test]
fn insert_mode_dual_mono_maps_to_zero() {
    assert_eq!(insert_mode_to_index(ChainInputMode::DualMono), 0);
}

#[test]
fn insert_mode_from_negative_index_defaults_to_mono() {
    assert_eq!(insert_mode_from_index(-1), ChainInputMode::Mono);
}

// --- normalized_chain_description ---

use super::chain_editor::normalized_chain_description;

#[test]
fn normalized_chain_description_trims_whitespace() {
    assert_eq!(
        normalized_chain_description("  Guitar 1  "),
        Some("Guitar 1".to_string())
    );
}

#[test]
fn normalized_chain_description_empty_returns_none() {
    assert_eq!(normalized_chain_description(""), None);
    assert_eq!(normalized_chain_description("   "), None);
}

// `preset_id_from_path` moved to
// `application::local_dispatcher_preset` in #555. The behaviour is
// covered there alongside the dispatcher tests; the GUI no longer
// owns the helper.

// --- project_title_for_path ---

use super::project_title_for_path;
use project::project::Project;

#[test]
fn project_title_uses_name_when_present() {
    let project = Project {
        name: Some("My Rig".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    assert_eq!(project_title_for_path(None, &project), "My Rig");
}

#[test]
fn project_title_falls_back_to_path_stem() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let path = std::path::PathBuf::from("/home/user/my_project.yaml");
    assert_eq!(project_title_for_path(Some(&path), &project), "my_project");
}

#[test]
fn project_title_empty_name_treated_as_absent() {
    let project = Project {
        name: Some("  ".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let path = std::path::PathBuf::from("/home/user/fallback.yaml");
    assert_eq!(project_title_for_path(Some(&path), &project), "fallback");
}

#[test]
fn project_title_no_name_no_path_empty_chains_is_novo_projeto() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    assert_eq!(project_title_for_path(None, &project), "Novo Projeto");
}

#[test]
fn project_title_no_name_no_path_with_chains_is_projeto() {
    let chain = Chain {
        id: ChainId("c".to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    };
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    };
    assert_eq!(project_title_for_path(None, &project), "Projeto");
}

// --- selected_device_index ---

use crate::audio_devices::selected_device_index;
use infra_cpal::AudioDeviceDescriptor;

#[test]
fn selected_device_index_finds_matching_device() {
    let devices = vec![
        AudioDeviceDescriptor {
            id: "dev_a".into(),
            name: "A".into(),
            channels: 2,
        },
        AudioDeviceDescriptor {
            id: "dev_b".into(),
            name: "B".into(),
            channels: 4,
        },
    ];
    assert_eq!(selected_device_index(&devices, Some("dev_b")), 1);
}

#[test]
fn selected_device_index_falls_back_to_zero_when_single_device() {
    // When there is exactly one device and the saved ID doesn't match,
    // auto-select it (index 0) so the user doesn't have to manually
    // pick the only option (common on single-device setups like Orange Pi).
    let devices = vec![AudioDeviceDescriptor {
        id: "dev_a".into(),
        name: "A".into(),
        channels: 2,
    }];
    assert_eq!(selected_device_index(&devices, Some("dev_x")), 0);
}

#[test]
fn selected_device_index_returns_negative_when_not_found_multiple_devices() {
    // When there are multiple devices and none match, return -1
    // so the UI shows "Select device" instead of picking one arbitrarily.
    let devices = vec![
        AudioDeviceDescriptor {
            id: "dev_a".into(),
            name: "A".into(),
            channels: 2,
        },
        AudioDeviceDescriptor {
            id: "dev_b".into(),
            name: "B".into(),
            channels: 4,
        },
    ];
    assert_eq!(selected_device_index(&devices, Some("dev_x")), -1);
}

#[test]
fn selected_device_index_returns_negative_for_none() {
    let devices = vec![AudioDeviceDescriptor {
        id: "dev_a".into(),
        name: "A".into(),
        channels: 2,
    }];
    assert_eq!(selected_device_index(&devices, None), -1);
}

// --- real_block_index_to_ui ---

use crate::project_view::real_block_index_to_ui;

#[test]
fn real_block_index_to_ui_maps_effect_blocks_correctly() {
    // [Input, Comp, Preamp, Delay, Output]
    let chain = test_chain(vec![
        input_kind(),
        effect_kind("dynamics"),
        effect_kind("preamp"),
        effect_kind("delay"),
        output_kind(),
    ]);
    assert_eq!(real_block_index_to_ui(&chain, 1), Some(0));
    assert_eq!(real_block_index_to_ui(&chain, 2), Some(1));
    assert_eq!(real_block_index_to_ui(&chain, 3), Some(2));
}

#[test]
fn real_block_index_to_ui_hidden_blocks_return_none() {
    let chain = test_chain(vec![input_kind(), effect_kind("delay"), output_kind()]);
    assert_eq!(real_block_index_to_ui(&chain, 0), None); // first input hidden
    assert_eq!(real_block_index_to_ui(&chain, 2), None); // last output hidden
}

#[test]
fn real_block_index_to_ui_out_of_range_returns_none() {
    let chain = test_chain(vec![input_kind(), output_kind()]);
    assert_eq!(real_block_index_to_ui(&chain, 99), None);
}

// --- project_display_name ---

use super::{project_display_name, UNTITLED_PROJECT_NAME};

#[test]
fn project_display_name_returns_trimmed_name() {
    let project = Project {
        name: Some("  My Project  ".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    assert_eq!(project_display_name(&project), "My Project");
}

#[test]
fn project_display_name_no_name_returns_untitled() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    assert_eq!(project_display_name(&project), UNTITLED_PROJECT_NAME);
}

#[test]
fn project_display_name_empty_name_returns_untitled() {
    let project = Project {
        name: Some("".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    assert_eq!(project_display_name(&project), UNTITLED_PROJECT_NAME);
}

// --- parse_cli_args_from additional edge cases ---

#[test]
fn parse_cli_args_auto_save_before_path() {
    let (path, auto_save, _) = parse_cli_args_from(&["openrig", "--auto-save", "/tmp/p.yaml"]);
    assert_eq!(path, Some(std::path::PathBuf::from("/tmp/p.yaml")));
    assert!(auto_save);
}

#[test]
fn parse_cli_args_multiple_paths_last_wins() {
    let (path, _, _) = parse_cli_args_from(&["openrig", "/first.yaml", "/second.yaml"]);
    assert_eq!(path, Some(std::path::PathBuf::from("/second.yaml")));
}

#[test]
fn parse_cli_args_dashed_flags_ignored_as_paths() {
    let (path, auto_save, fullscreen) = parse_cli_args_from(&["openrig", "--verbose", "--debug"]);
    assert_eq!(path, None);
    assert!(!auto_save);
    assert!(!fullscreen);
}

// --- chain_endpoint_label ---

use super::project_view::chain_endpoint_label;

#[test]
fn chain_endpoint_label_returns_prefix() {
    assert_eq!(chain_endpoint_label("In", &[0, 1]), "In");
    assert_eq!(chain_endpoint_label("Out", &[]), "Out");
}

#[test]
fn open_cli_project_errors_on_nonexistent_path() {
    let result = open_cli_project(&std::path::PathBuf::from("/nonexistent/project.yaml"));
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("does not exist"), "got: {}", msg);
}

#[test]
fn parse_cli_args_extracts_path_and_auto_save_flag() {
    let (path, auto_save, fullscreen) = parse_cli_args_from(&["openrig", "/tmp/project.yaml"]);
    assert_eq!(path, Some(std::path::PathBuf::from("/tmp/project.yaml")));
    assert!(!auto_save);
    assert!(!fullscreen);

    let (path, auto_save, _) = parse_cli_args_from(&["openrig", "--auto-save"]);
    assert_eq!(path, None);
    assert!(auto_save);

    let (path, auto_save, _) =
        parse_cli_args_from(&["openrig", "/tmp/project.yaml", "--auto-save"]);
    assert_eq!(path, Some(std::path::PathBuf::from("/tmp/project.yaml")));
    assert!(auto_save);

    let (path, auto_save, _) = parse_cli_args_from(&["openrig"]);
    assert_eq!(path, None);
    assert!(!auto_save);

    let (path, auto_save, _) = parse_cli_args_from(&["openrig", "--unknown-flag"]);
    assert_eq!(path, None);
    assert!(!auto_save);
}

#[test]
fn parse_cli_args_fullscreen_flag() {
    let (_, _, fullscreen) = parse_cli_args_from(&["openrig", "--fullscreen"]);
    assert!(fullscreen);

    let (path, auto_save, fullscreen) =
        parse_cli_args_from(&["openrig", "--fullscreen", "--auto-save", "/tmp/p.yaml"]);
    assert_eq!(path, Some(std::path::PathBuf::from("/tmp/p.yaml")));
    assert!(auto_save);
    assert!(fullscreen);

    let (_, _, fullscreen) = parse_cli_args_from(&["openrig", "/tmp/p.yaml"]);
    assert!(!fullscreen);
}

