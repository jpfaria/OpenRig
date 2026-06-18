use super::*;
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::device::DeviceSettings;

fn input_entry(device: &str, channels: Vec<usize>, mode: ChainInputMode) -> InputEntry {
    InputEntry {
        device_id: DeviceId(device.into()),
        mode,
        channels,
    }
}

fn input_block(entries: Vec<InputEntry>) -> AudioBlock {
    AudioBlock {
        id: BlockId("chain:0:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries,
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn chain_with_input(id: &str, enabled: bool, entries: Vec<InputEntry>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("Guitar".into()),
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        blocks: vec![input_block(entries)],
    }
}

fn project_from_chain(chain: Chain) -> Project {
    Project {
        name: None,
        device_settings: Vec::<DeviceSettings>::new(),
        chains: vec![chain],
        midi: None,
    }
}

#[test]
fn fingerprint_skips_disabled_chains() {
    let entries = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
    let fp_enabled = project_stream_fingerprint(&project_from_chain(chain_with_input(
        "chain:0",
        true,
        entries.clone(),
    )));
    let fp_disabled = project_stream_fingerprint(&project_from_chain(chain_with_input(
        "chain:0", false, entries,
    )));
    assert_ne!(fp_enabled, fp_disabled);
    assert!(fp_disabled.is_empty());
}

#[test]
fn fingerprint_changes_when_input_mode_changes() {
    let mono = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
    let stereo = vec![input_entry("dev:1", vec![0, 1], ChainInputMode::Stereo)];

    let fp_mono =
        project_stream_fingerprint(&project_from_chain(chain_with_input("chain:0", true, mono)));
    let fp_stereo = project_stream_fingerprint(&project_from_chain(chain_with_input(
        "chain:0", true, stereo,
    )));
    assert_ne!(fp_mono, fp_stereo);
}

#[test]
fn fingerprint_changes_when_device_id_changes() {
    let dev_a = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
    let dev_b = vec![input_entry("dev:2", vec![0], ChainInputMode::Mono)];

    let fp_a = project_stream_fingerprint(&project_from_chain(chain_with_input(
        "chain:0", true, dev_a,
    )));
    let fp_b = project_stream_fingerprint(&project_from_chain(chain_with_input(
        "chain:0", true, dev_b,
    )));
    assert_ne!(fp_a, fp_b);
}

#[test]
fn fingerprint_stable_for_identical_projects() {
    let mk = || {
        project_from_chain(chain_with_input(
            "chain:0",
            true,
            vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)],
        ))
    };
    assert_eq!(
        project_stream_fingerprint(&mk()),
        project_stream_fingerprint(&mk())
    );
}

#[test]
fn short_device_label_strips_backend_prefix() {
    assert_eq!(
        short_device_label("coreaudio:Built-in Output"),
        "Built-in Output"
    );
    assert_eq!(
        short_device_label("jack:system:playback_1"),
        "system:playback_1"
    );
    assert_eq!(short_device_label("plain-device"), "plain-device");
}

// ── output_endpoint_labels_for_project — per-binding enumeration (#716) ──

fn binding_output_block(io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("chain:0:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![],
            io: io.to_string(),
            endpoint: endpoint.to_string(),
        }),
    }
}

fn chain_with_binding_output(id: &str, io: &str, endpoint: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        blocks: vec![binding_output_block(io, endpoint)],
    }
}

fn legacy_output_block(device: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("chain:0:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn chain_with_legacy_output(id: &str, device: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        blocks: vec![legacy_output_block(device)],
    }
}

/// A project with two chains each referencing a distinct binding output must
/// yield two separate spectrum analyzer labels — one per (binding_id, endpoint).
#[test]
fn spectrum_enumerates_one_per_binding_output_endpoint() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            chain_with_binding_output("chain:A", "binding-A", "Speaker"),
            chain_with_binding_output("chain:B", "binding-B", "Speaker"),
        ],
        midi: None,
    };

    let labels = output_endpoint_labels_for_project(&project);

    assert_eq!(
        labels.len(),
        2,
        "two binding-output chains → 2 spectrum analyzers; got {:?}",
        labels
    );
    assert!(
        labels[0] != labels[1],
        "distinct binding outputs must have distinct labels; both are {:?}",
        labels[0]
    );
}

/// Same output endpoint name across two different bindings must still appear
/// as two separate spectrum rows — no cross-binding deduplication.
#[test]
fn spectrum_does_not_merge_same_endpoint_from_different_bindings() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            chain_with_binding_output("chain:A", "binding-A", "Out1"),
            chain_with_binding_output("chain:B", "binding-B", "Out1"),
        ],
        midi: None,
    };

    let labels = output_endpoint_labels_for_project(&project);

    assert_eq!(
        labels.len(),
        2,
        "same endpoint name, different bindings → 2 spectrum rows; got {:?}",
        labels
    );
}

/// Legacy chains (io == "") must still work — the spectrum shows one analyzer
/// per legacy output entry (physical device reference), not zero.
#[test]
fn spectrum_legacy_entries_path_still_enumerates_per_entry() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain_with_legacy_output("chain:0", "coreaudio:Built-in Output")],
        midi: None,
    };

    let labels = output_endpoint_labels_for_project(&project);

    assert!(
        !labels.is_empty(),
        "legacy output entry must yield at least one spectrum row"
    );
}
