use super::{
    block_drawer_state, chain_routing_summary, insertion_slot_indices, resolve_block_io_endpoint,
    ui_bindings, BlockDrawerMode,
};
use domain::ids::{ChainId, DeviceId};
use infra_filesystem::{AppConfig, ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

// ── ui_bindings projector tests (#716) ───────────────────────────────────────

#[test]
fn ui_bindings_projects_registry_two_bindings() {
    let config = AppConfig {
        io_bindings: vec![
            IoBinding {
                id: "main".into(),
                name: "Scarlett 2i2".into(),
                inputs: vec![IoEndpoint {
                    name: "Guitar In 1".into(),
                    device_id: DeviceId("dev-001".into()),
                    mode: ChannelMode::Mono,
                    channels: vec![0],
                }],
                outputs: vec![IoEndpoint {
                    name: "Monitor Out".into(),
                    device_id: DeviceId("dev-001".into()),
                    mode: ChannelMode::Stereo,
                    channels: vec![0, 1],
                }],
            },
            IoBinding {
                id: "loop".into(),
                name: "Effects Loop".into(),
                inputs: vec![],
                outputs: vec![IoEndpoint {
                    name: "Send".into(),
                    device_id: DeviceId("dev-002".into()),
                    mode: ChannelMode::Mono,
                    channels: vec![2],
                }],
            },
        ],
        ..Default::default()
    };

    let models = ui_bindings(&config);

    assert_eq!(models.len(), 2);

    // First binding
    assert_eq!(models[0].id, "main");
    assert_eq!(models[0].name, "Scarlett 2i2");
    assert_eq!(models[0].inputs.len(), 1);
    assert_eq!(models[0].inputs[0].name, "Guitar In 1");
    assert_eq!(models[0].inputs[0].device_label, "dev-001");
    assert_eq!(models[0].inputs[0].mode, "mono");
    assert_eq!(models[0].inputs[0].channels_label, "1");
    assert_eq!(models[0].outputs.len(), 1);
    assert_eq!(models[0].outputs[0].name, "Monitor Out");
    assert_eq!(models[0].outputs[0].channels_label, "1, 2");

    // Second binding
    assert_eq!(models[1].id, "loop");
    assert_eq!(models[1].name, "Effects Loop");
    assert_eq!(models[1].inputs.len(), 0);
    assert_eq!(models[1].outputs.len(), 1);
    assert_eq!(models[1].outputs[0].name, "Send");
    assert_eq!(models[1].outputs[0].channels_label, "3");
}

#[test]
fn ui_bindings_empty_config_returns_empty_vec() {
    let config = AppConfig {
        io_bindings: vec![],
        ..Default::default()
    };
    assert!(ui_bindings(&config).is_empty());
}

// ── resolve_block_io_endpoint tests (#716) ────────────────────────────────────

#[test]
fn block_ref_resolves_to_endpoint_model() {
    let config = AppConfig {
        io_bindings: vec![IoBinding {
            id: "main".into(),
            name: "Scarlett 2i2".into(),
            inputs: vec![IoEndpoint {
                name: "In1".into(),
                device_id: DeviceId("dev-001".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![],
        }],
        ..Default::default()
    };

    let result = resolve_block_io_endpoint(&config, "main", "In1");
    assert!(result.is_some(), "expected Some for known binding/endpoint");
    let ep = result.unwrap();
    assert_eq!(ep.name, "In1");
    assert_eq!(ep.device_label, "dev-001");
}

#[test]
fn unbound_block_resolves_to_none() {
    let config = AppConfig {
        io_bindings: vec![],
        ..Default::default()
    };
    // Empty io string means no binding selected.
    assert!(resolve_block_io_endpoint(&config, "", "").is_none());
}

#[test]
fn missing_endpoint_name_resolves_to_none() {
    let config = AppConfig {
        io_bindings: vec![IoBinding {
            id: "main".into(),
            name: "Scarlett 2i2".into(),
            inputs: vec![IoEndpoint {
                name: "In1".into(),
                device_id: DeviceId("dev-001".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![],
        }],
        ..Default::default()
    };
    // Binding found but endpoint name doesn't match anything.
    assert!(resolve_block_io_endpoint(&config, "main", "NonExistent").is_none());
}

#[test]
fn insertion_slots_cover_edges_and_between_positions() {
    assert_eq!(insertion_slot_indices(0), vec![0]);
    assert_eq!(insertion_slot_indices(3), vec![0, 1, 2, 3]);
}

#[test]
fn block_drawer_labels_match_add_mode() {
    let state = block_drawer_state(None, "delay", Some("digital_clean"));

    assert_eq!(state.mode, BlockDrawerMode::Add);
    assert_eq!(state.title, "");
    assert_eq!(state.confirm_label, "Adicionar");
}

#[test]
fn block_drawer_labels_match_edit_mode() {
    let state = block_drawer_state(Some(2), "delay", Some("digital_clean"));

    assert_eq!(state.mode, BlockDrawerMode::Edit);
    assert_eq!(state.title, "");
    assert_eq!(state.confirm_label, "Salvar");
}

// --- accent_color_for_icon_kind ---

use super::{accent_color_for_icon_kind, block_family_for_kind, icon_index_for_icon_kind};

#[test]
fn accent_color_returns_distinct_color_for_each_known_kind() {
    let kinds = [
        "preamp",
        "amp",
        "cab",
        "body",
        "ir",
        "full_rig",
        "gain",
        "dynamics",
        "filter",
        "wah",
        "modulation",
        "delay",
        "reverb",
        "utility",
        "nam",
        "pitch",
        "insert",
        "input",
        "output",
    ];
    for kind in &kinds {
        let color = accent_color_for_icon_kind(kind);
        assert_eq!(color.alpha(), 255, "alpha must be 255 for kind '{}'", kind);
    }
}

#[test]
fn accent_color_returns_fallback_for_unknown_kind() {
    let fallback = accent_color_for_icon_kind("nonexistent_kind");
    let expected = slint::Color::from_argb_u8(255, 0x7f, 0xb0, 0xff);
    assert_eq!(fallback, expected);
}

#[test]
fn accent_color_preamp_is_orange() {
    let c = accent_color_for_icon_kind("preamp");
    assert_eq!(c, slint::Color::from_argb_u8(255, 0xf2, 0x9f, 0x38));
}

#[test]
fn accent_color_input_output_share_same_color() {
    assert_eq!(
        accent_color_for_icon_kind("input"),
        accent_color_for_icon_kind("output"),
    );
}

// --- icon_index_for_icon_kind ---

#[test]
fn icon_index_returns_correct_index_for_known_kinds() {
    assert_eq!(icon_index_for_icon_kind("preamp"), 0);
    assert_eq!(icon_index_for_icon_kind("amp"), 1);
    assert_eq!(icon_index_for_icon_kind("cab"), 2);
    assert_eq!(icon_index_for_icon_kind("body"), 3);
    assert_eq!(icon_index_for_icon_kind("ir"), 4);
    assert_eq!(icon_index_for_icon_kind("full_rig"), 5);
    assert_eq!(icon_index_for_icon_kind("gain"), 6);
    assert_eq!(icon_index_for_icon_kind("dynamics"), 7);
    assert_eq!(icon_index_for_icon_kind("filter"), 8);
    assert_eq!(icon_index_for_icon_kind("wah"), 9);
    assert_eq!(icon_index_for_icon_kind("modulation"), 10);
    assert_eq!(icon_index_for_icon_kind("delay"), 11);
    assert_eq!(icon_index_for_icon_kind("reverb"), 12);
    assert_eq!(icon_index_for_icon_kind("utility"), 13);
    assert_eq!(icon_index_for_icon_kind("nam"), 14);
    assert_eq!(icon_index_for_icon_kind("pitch"), 15);
}

#[test]
fn icon_index_unknown_falls_back_to_utility() {
    assert_eq!(icon_index_for_icon_kind("unknown"), 13);
    assert_eq!(icon_index_for_icon_kind(""), 13);
}

// --- block_family_for_kind ---

#[test]
fn block_family_groups_amp_related_kinds() {
    assert_eq!(block_family_for_kind("preamp"), "amp");
    assert_eq!(block_family_for_kind("amp"), "amp");
    assert_eq!(block_family_for_kind("full_rig"), "amp");
    assert_eq!(block_family_for_kind("nam"), "amp");
}

#[test]
fn block_family_groups_space_kinds() {
    assert_eq!(block_family_for_kind("delay"), "space");
    assert_eq!(block_family_for_kind("reverb"), "space");
}

#[test]
fn block_family_groups_routing_kinds() {
    assert_eq!(block_family_for_kind("input"), "routing");
    assert_eq!(block_family_for_kind("output"), "routing");
    assert_eq!(block_family_for_kind("insert"), "routing");
}

#[test]
fn block_family_returns_individual_families() {
    assert_eq!(block_family_for_kind("cab"), "cab");
    assert_eq!(block_family_for_kind("body"), "body");
    assert_eq!(block_family_for_kind("ir"), "ir");
    assert_eq!(block_family_for_kind("gain"), "gain");
    assert_eq!(block_family_for_kind("dynamics"), "dynamics");
    assert_eq!(block_family_for_kind("filter"), "filter");
    assert_eq!(block_family_for_kind("wah"), "wah");
    assert_eq!(block_family_for_kind("pitch"), "pitch");
    assert_eq!(block_family_for_kind("modulation"), "modulation");
    assert_eq!(block_family_for_kind("utility"), "utility");
}

#[test]
fn block_family_unknown_falls_back_to_utility() {
    assert_eq!(block_family_for_kind("unknown_kind"), "utility");
    assert_eq!(block_family_for_kind(""), "utility");
}

// --- block_drawer_state edge cases ---

#[test]
fn block_drawer_state_add_mode_without_model_id() {
    let state = block_drawer_state(None, "reverb", None);
    assert_eq!(state.mode, BlockDrawerMode::Add);
    assert_eq!(state.effect_type, "reverb");
    assert!(state.model_id.is_none());
}

#[test]
fn block_drawer_state_edit_mode_preserves_model_id() {
    let state = block_drawer_state(Some(0), "gain", Some("ts9"));
    assert_eq!(state.mode, BlockDrawerMode::Edit);
    assert_eq!(state.model_id, Some("ts9".to_string()));
}

#[test]
fn routing_summary_uses_human_friendly_channel_numbers() {
    // #716: the chain's I/O channels resolve from the binding registry, not
    // from block `entries`. The chain references `io1`; the registry binding
    // carries a mono input on ch 0 and a stereo output on ch 0,1.
    let chain = Chain {
        id: ChainId("chain:1".to_string()),
        description: Some("Guitarra".to_string()),
        instrument: block_core::INST_ELECTRIC_GUITAR.to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io1".to_string()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    let registry = vec![IoBinding {
        id: "io1".into(),
        name: "IO1".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("in".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];

    assert_eq!(
        chain_routing_summary(&chain, &registry),
        "Entrada 1 -> Saida 1, 2".to_string()
    );
}
