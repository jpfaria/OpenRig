//! Recent-projects / tooltip / block-index lib tests (issue #792 split from
//! lib_tests.rs). Shares chain fixtures via super::tests.

use super::tests::{delay_model_ids, effect_kind, input_kind, output_kind, test_chain};
use super::*;
use crate::project_ops::build_device_settings_from_gui;
use infra_cpal::AudioDeviceDescriptor;
use domain::ids::{BlockId, DeviceId};
use project::chain::Chain;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::project::Project;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};

// --- sync_recent_projects ---

use super::project_ops::sync_recent_projects;
use infra_filesystem::{AppConfig, RecentProjectEntry};

#[test]
fn sync_recent_projects_deduplicates_by_canonical_path() {
    let mut config = AppConfig {
        recent_projects: vec![
            RecentProjectEntry {
                project_path: "/tmp/project_a.yaml".to_string(),
                project_name: "A".to_string(),
                is_valid: true,
                invalid_reason: None,
            },
            RecentProjectEntry {
                project_path: "/tmp/project_a.yaml".to_string(),
                project_name: "A duplicate".to_string(),
                is_valid: true,
                invalid_reason: None,
            },
        ],
        ..Default::default()
    };
    let changed = sync_recent_projects(&mut config);
    assert!(changed);
    assert_eq!(config.recent_projects.len(), 1);
    assert_eq!(config.recent_projects[0].project_name, "A");
}

#[test]
fn sync_recent_projects_empty_name_becomes_untitled() {
    let mut config = AppConfig {
        recent_projects: vec![RecentProjectEntry {
            project_path: "/tmp/x.yaml".to_string(),
            project_name: "  ".to_string(),
            is_valid: true,
            invalid_reason: None,
        }],
        ..Default::default()
    };
    sync_recent_projects(&mut config);
    assert_eq!(
        config.recent_projects[0].project_name,
        UNTITLED_PROJECT_NAME
    );
}

#[test]
fn sync_recent_projects_returns_false_when_unchanged() {
    let mut config = AppConfig {
        recent_projects: vec![RecentProjectEntry {
            project_path: "/tmp/project.yaml".to_string(),
            project_name: "My Project".to_string(),
            is_valid: true,
            invalid_reason: None,
        }],
        ..Default::default()
    };
    let changed = sync_recent_projects(&mut config);
    assert!(!changed);
}

// --- register_recent_project ---

use super::register_recent_project;

#[test]
fn register_recent_project_adds_to_front() {
    let mut config = AppConfig {
        recent_projects: vec![RecentProjectEntry {
            project_path: "/old.yaml".to_string(),
            project_name: "Old".to_string(),
            is_valid: true,
            invalid_reason: None,
        }],
        ..Default::default()
    };
    register_recent_project(&mut config, &std::path::PathBuf::from("/new.yaml"), "New");
    assert_eq!(config.recent_projects.len(), 2);
    assert_eq!(config.recent_projects[0].project_name, "New");
}

#[test]
fn register_recent_project_removes_duplicate_and_reinserts_at_front() {
    let path = std::path::PathBuf::from("/project.yaml");
    let mut config = AppConfig {
        recent_projects: vec![
            RecentProjectEntry {
                project_path: "/other.yaml".to_string(),
                project_name: "Other".to_string(),
                is_valid: true,
                invalid_reason: None,
            },
            RecentProjectEntry {
                project_path: "/project.yaml".to_string(),
                project_name: "Project".to_string(),
                is_valid: true,
                invalid_reason: None,
            },
        ],
        ..Default::default()
    };
    register_recent_project(&mut config, &path, "Updated");
    assert_eq!(config.recent_projects.len(), 2);
    assert_eq!(config.recent_projects[0].project_name, "Updated");
    assert_eq!(config.recent_projects[1].project_name, "Other");
}

#[test]
fn register_recent_project_empty_name_becomes_untitled() {
    let mut config = AppConfig::default();
    register_recent_project(&mut config, &std::path::PathBuf::from("/x.yaml"), "  ");
    assert_eq!(
        config.recent_projects[0].project_name,
        UNTITLED_PROJECT_NAME
    );
}

// --- mark_recent_project_invalid ---

use crate::project_ops::mark_recent_project_invalid;

#[test]
fn mark_recent_project_invalid_sets_flag_and_reason() {
    let mut config = AppConfig {
        recent_projects: vec![RecentProjectEntry {
            project_path: "/p.yaml".to_string(),
            project_name: "P".to_string(),
            is_valid: true,
            invalid_reason: None,
        }],
        ..Default::default()
    };
    mark_recent_project_invalid(
        &mut config,
        &std::path::PathBuf::from("/p.yaml"),
        "File corrupted",
    );
    assert!(!config.recent_projects[0].is_valid);
    assert_eq!(
        config.recent_projects[0].invalid_reason.as_deref(),
        Some("File corrupted")
    );
}

#[test]
fn mark_recent_project_invalid_empty_reason_gets_default() {
    let mut config = AppConfig {
        recent_projects: vec![RecentProjectEntry {
            project_path: "/p.yaml".to_string(),
            project_name: "P".to_string(),
            is_valid: true,
            invalid_reason: None,
        }],
        ..Default::default()
    };
    mark_recent_project_invalid(&mut config, &std::path::PathBuf::from("/p.yaml"), "  ");
    assert!(!config.recent_projects[0].is_valid);
    assert_eq!(
        config.recent_projects[0].invalid_reason.as_deref(),
        Some("Projeto inválido")
    );
}

#[test]
fn mark_recent_project_invalid_nonexistent_path_does_nothing() {
    let mut config = AppConfig {
        recent_projects: vec![RecentProjectEntry {
            project_path: "/p.yaml".to_string(),
            project_name: "P".to_string(),
            is_valid: true,
            invalid_reason: None,
        }],
        ..Default::default()
    };
    mark_recent_project_invalid(&mut config, &std::path::PathBuf::from("/other.yaml"), "err");
    assert!(config.recent_projects[0].is_valid);
    assert!(config.recent_projects[0].invalid_reason.is_none());
}

// --- chain_inputs_tooltip / chain_outputs_tooltip ---
//
// #716: device endpoints resolve from the per-machine I/O binding registry,
// not from block `entries`. A chain references its I/O via `io_binding_ids`;
// the tooltip helpers take that registry and resolve through
// `engine::runtime_endpoints::resolve_chain_io`.

use super::project_view::{chain_inputs_tooltip, chain_outputs_tooltip};

/// A chain that selects binding `"io"` (head input + tail output resolved from
/// the registry) and carries only structural head/tail device blocks.
fn tooltip_chain() -> Chain {
    let mut chain = test_chain(vec![input_kind(), output_kind()]);
    chain.io_binding_ids = vec!["io".into()];
    chain
}

/// Registry binding mirroring the legacy `input_kind()` (mono, dev "dev",
/// ch [0]) + `output_kind()` (stereo, dev "dev", ch [0, 1]).
fn tooltip_registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn empty_project() -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }
}

#[test]
fn chain_inputs_tooltip_shows_device_name_and_channels() {
    let chain = tooltip_chain();
    let project = empty_project();
    let devices = vec![AudioDeviceDescriptor {
        id: "dev".into(),
        name: "USB Audio".into(),
        channels: 2,
    }];
    let tooltip = chain_inputs_tooltip(&chain, &project, &devices, &tooltip_registry());
    assert!(
        tooltip.contains("USB Audio"),
        "tooltip should contain device name: {}",
        tooltip
    );
    assert!(
        tooltip.contains("Mono"),
        "tooltip should contain mode: {}",
        tooltip
    );
    assert!(
        tooltip.contains("1"),
        "tooltip should contain channel number: {}",
        tooltip
    );
}

#[test]
fn chain_inputs_tooltip_no_input_block() {
    // No binding selected ⇒ no resolved inputs.
    let chain = test_chain(vec![effect_kind("delay")]);
    let project = empty_project();
    let tooltip = chain_inputs_tooltip(&chain, &project, &[], &[]);
    assert_eq!(tooltip, "No input configured");
}

#[test]
fn chain_inputs_tooltip_unknown_device_shows_id() {
    let chain = tooltip_chain();
    let project = empty_project();
    // No devices → falls back to device_id from the resolved binding endpoint.
    let tooltip = chain_inputs_tooltip(&chain, &project, &[], &tooltip_registry());
    assert!(
        tooltip.contains("dev"),
        "should fall back to device id: {}",
        tooltip
    );
}

#[test]
fn chain_outputs_tooltip_shows_device_and_channels() {
    let chain = tooltip_chain();
    let project = empty_project();
    let devices = vec![AudioDeviceDescriptor {
        id: "dev".into(),
        name: "Headphones".into(),
        channels: 2,
    }];
    let tooltip = chain_outputs_tooltip(&chain, &project, &devices, &tooltip_registry());
    assert!(
        tooltip.contains("Headphones"),
        "should contain device name: {}",
        tooltip
    );
    assert!(
        tooltip.contains("Stereo"),
        "should contain mode: {}",
        tooltip
    );
}

#[test]
fn chain_outputs_tooltip_no_output_block() {
    // No binding selected ⇒ no resolved outputs.
    let chain = test_chain(vec![effect_kind("delay")]);
    let project = empty_project();
    let tooltip = chain_outputs_tooltip(&chain, &project, &[], &[]);
    assert_eq!(tooltip, "No output configured");
}

// --- block_model_index ---

use crate::project_view::block_model_index;

#[test]
fn block_model_index_finds_known_delay_model() {
    let models = delay_model_ids();
    let first = &models[0];
    let idx = block_model_index("delay", first, "electric_guitar");
    assert_eq!(idx, 0);
}

#[test]
fn block_model_index_unknown_model_returns_negative() {
    let idx = block_model_index("delay", "nonexistent_model", "electric_guitar");
    assert_eq!(idx, -1);
}

// --- block_type_index ---

use crate::project_view::block_type_index;

#[test]
fn block_type_index_finds_delay() {
    let idx = block_type_index("delay", "electric_guitar");
    assert!(idx >= 0, "delay should be in type picker");
}

#[test]
fn block_type_index_unknown_type_returns_negative() {
    let idx = block_type_index("nonexistent_type", "electric_guitar");
    assert_eq!(idx, -1);
}

#[test]
fn block_type_index_input_is_present() {
    let idx = block_type_index("input", "electric_guitar");
    assert!(idx >= 0, "input should be in type picker");
}

#[test]
fn block_type_index_output_is_present() {
    let idx = block_type_index("output", "electric_guitar");
    assert!(idx >= 0, "output should be in type picker");
}

#[test]
fn block_type_index_insert_is_present() {
    let idx = block_type_index("insert", "electric_guitar");
    assert!(idx >= 0, "insert should be in type picker");
}

#[test]
fn build_device_settings_deduplicates_same_device_id() {
    use infra_filesystem::GuiAudioDeviceSettings;
    let input = vec![GuiAudioDeviceSettings {
        device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
        name: "TEYUN Q26".into(),
        sample_rate: 48000,
        buffer_size_frames: 64,
        bit_depth: 16,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }];
    let output = vec![GuiAudioDeviceSettings {
        device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
        name: "TEYUN Q26".into(),
        sample_rate: 48000,
        buffer_size_frames: 64,
        bit_depth: 16,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }];
    let result = build_device_settings_from_gui(&input, &output);
    assert_eq!(
        result.len(),
        1,
        "same device_id in input+output should produce 1 entry"
    );
    assert_eq!(result[0].device_id.0, "alsa:hw:CARD=Q26,DEV=0");
}

#[test]
fn build_device_settings_keeps_distinct_devices() {
    use infra_filesystem::GuiAudioDeviceSettings;
    let input = vec![GuiAudioDeviceSettings {
        device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
        name: "TEYUN Q26".into(),
        sample_rate: 48000,
        buffer_size_frames: 64,
        bit_depth: 16,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }];
    let output = vec![GuiAudioDeviceSettings {
        device_id: "alsa:hw:CARD=hdmi0,DEV=0".into(),
        name: "HDMI".into(),
        sample_rate: 48000,
        buffer_size_frames: 128,
        bit_depth: 24,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }];
    let result = build_device_settings_from_gui(&input, &output);
    assert_eq!(
        result.len(),
        2,
        "different device_ids should produce 2 entries"
    );
}

#[test]
fn build_device_settings_input_takes_precedence_on_duplicate() {
    use infra_filesystem::GuiAudioDeviceSettings;
    let input = vec![GuiAudioDeviceSettings {
        device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
        name: "TEYUN Q26".into(),
        sample_rate: 48000,
        buffer_size_frames: 128,
        bit_depth: 24,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }];
    let output = vec![GuiAudioDeviceSettings {
        device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
        name: "TEYUN Q26".into(),
        sample_rate: 44100,
        buffer_size_frames: 64,
        bit_depth: 16,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }];
    let result = build_device_settings_from_gui(&input, &output);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].sample_rate, 48000,
        "input settings should take precedence"
    );
    assert_eq!(result[0].buffer_size_frames, 128);
}

// #436 #1: the app's load path runs the NEW rig engine (GUI unchanged).

#[test]
fn rig_project_for_routes_legacy_through_rig_engine() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("project.yaml");
    std::fs::write(
        &path,
        "name: t\nchains:\n\
         - description: Guitarra\n  instrument: electric_guitar\n  volume: 137.0\n  blocks:\n\
         \x20 - type: input\n    enabled: true\n    model: standard\n    entries:\n\
         \x20   - device_id: dev\n      mode: mono\n      channels: [0]\n\
         \x20 - type: gain\n    enabled: true\n    model: volume\n    params: { volume: 80.0, mute: false }\n\
         \x20 - type: output\n    enabled: true\n    model: standard\n    entries:\n\
         \x20   - device_id: dev\n      mode: stereo\n      channels: [0, 1]\n",
    )
    .unwrap();

    let (_rig, proj) = crate::project_ops::load_rig_and_project(&path).expect("rig load");

    assert_eq!(proj.chains.len(), 1, "rig input shown as a chain");
    assert_eq!(
        proj.chains[0].id.0, "rig:input-1",
        "GUI sees rig input as a chain"
    );
    assert!(
        !proj.chains[0].enabled,
        "nothing auto-starts — the user enables it"
    );
    assert_eq!(
        proj.chains[0].volume, 137.0,
        "preset volume preserved through the rig path (invariant #10)"
    );
    assert!(
        !path.with_extension("openrig").exists(),
        "#716: legacy .yaml migrates IN MEMORY — no .openrig sibling is written"
    );
    assert!(path.exists(), "the .yaml project file stays in place");
}

#[test]
fn chain_block_item_flags_unavailable_model_for_the_tile() {
    use crate::project_view::chain_block_item_from_block;
    use project::param::ParameterSet;

    // The builder resolves thumbnail asset paths; init them (idempotent).
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());

    let gain = |id: &str, model: &str| AudioBlock {
        id: BlockId(id.into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    };

    // `ibanez_ts9` is a native gain model → resolvable. The `nam_` id has no
    // pack on disk → unavailable. The tile uses this flag to show the block
    // as deactivated and block its enable toggle (#606).
    assert!(
        !chain_block_item_from_block(&gain("a", "ibanez_ts9")).unavailable,
        "a resolvable model must not be flagged unavailable"
    );
    assert!(
        chain_block_item_from_block(&gain("u", "nam_uninstalled_pedal_for_issue_606")).unavailable,
        "BUG #606: a block whose model is uninstalled must be flagged unavailable \
         so the tile can show it and block its enable toggle"
    );
}
