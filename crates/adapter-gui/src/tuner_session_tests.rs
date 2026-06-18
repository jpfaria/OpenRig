use super::*;
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry};
use project::chain::{Chain, ChainInputMode};
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

// ── freq_to_octave ───────────────────────────────────────────────────────

#[test]
fn freq_to_octave_returns_zero_for_non_positive_freq() {
    assert_eq!(freq_to_octave(0.0, 440.0), 0);
    assert_eq!(freq_to_octave(-12.0, 440.0), 0);
}

#[test]
fn freq_to_octave_a4_440_is_octave_4() {
    assert_eq!(freq_to_octave(440.0, 440.0), 4);
}

#[test]
fn freq_to_octave_low_e_is_octave_2() {
    // E2 ≈ 82.41 Hz on standard tuning.
    assert_eq!(freq_to_octave(82.41, 440.0), 2);
}

#[test]
fn freq_to_octave_high_a5_is_octave_5() {
    assert_eq!(freq_to_octave(880.0, 440.0), 5);
}

#[test]
fn freq_to_octave_uses_reference_pitch_for_anchor() {
    // With reference 432 Hz, a frequency of 432 still anchors to A4.
    assert_eq!(freq_to_octave(432.0, 432.0), 4);
}

// ── placeholder_row ──────────────────────────────────────────────────────

#[test]
fn placeholder_row_has_label_and_inactive_defaults() {
    let row = placeholder_row("CHAIN · IN 1 · CH 1".into());
    assert_eq!(row.label, "CHAIN · IN 1 · CH 1");
    assert_eq!(row.note, "—");
    assert_eq!(row.octave, 0);
    assert_eq!(row.cents, 0.0);
    assert_eq!(row.frequency, 0.0);
    assert!(!row.active);
}

// ── project_input_fingerprint ────────────────────────────────────────────

#[test]
fn fingerprint_skips_disabled_chains() {
    let entries = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
    let fp_enabled = project_input_fingerprint(&project_from_chain(chain_with_input(
        "chain:0",
        true,
        entries.clone(),
    )));
    let fp_disabled = project_input_fingerprint(&project_from_chain(chain_with_input(
        "chain:0", false, entries,
    )));

    assert_ne!(fp_enabled, fp_disabled);
    assert!(
        fp_disabled.is_empty(),
        "disabled chains contribute nothing to the fingerprint, got {:?}",
        fp_disabled
    );
}

#[test]
fn fingerprint_changes_when_channels_change() {
    let mono = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
    let stereo = vec![input_entry("dev:1", vec![0, 1], ChainInputMode::Stereo)];

    let fp_mono =
        project_input_fingerprint(&project_from_chain(chain_with_input("chain:0", true, mono)));
    let fp_stereo = project_input_fingerprint(&project_from_chain(chain_with_input(
        "chain:0", true, stereo,
    )));

    assert_ne!(fp_mono, fp_stereo);
}

#[test]
fn fingerprint_changes_when_device_id_changes() {
    let dev_a = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
    let dev_b = vec![input_entry("dev:2", vec![0], ChainInputMode::Mono)];

    let fp_a = project_input_fingerprint(&project_from_chain(chain_with_input(
        "chain:0", true, dev_a,
    )));
    let fp_b = project_input_fingerprint(&project_from_chain(chain_with_input(
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
            vec![input_entry("dev:1", vec![0, 1], ChainInputMode::Stereo)],
        ))
    };

    assert_eq!(
        project_input_fingerprint(&mk()),
        project_input_fingerprint(&mk())
    );
}

// ── RowState ─────────────────────────────────────────────────────────────

#[test]
fn row_state_starts_with_empty_buffer() {
    let ring = Arc::new(SpscRing::new(BUFFER_SIZE * 2, 0.0_f32));
    let state = RowState::new(ring, 48_000, REFERENCE_HZ);
    assert!(state.sample_buf.is_empty());
    assert!(state.sample_buf.capacity() >= BUFFER_SIZE * 2);
}

// ── input_port_labels_for_project — per-binding enumeration (#716) ───────

fn binding_input_block(io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("chain:0:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![],
            io: io.to_string(),
            endpoint: endpoint.to_string(),
        }),
    }
}

fn chain_with_binding_input(id: &str, io: &str, endpoint: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        blocks: vec![binding_input_block(io, endpoint)],
    }
}

/// A chain with two bindings each contributing one input port must yield two
/// separate placeholder rows — one per (binding_id, endpoint) pair, not merged.
#[test]
fn tuner_enumerates_one_per_binding_input_port() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            chain_with_binding_input("chain:A", "binding-A", "Guitar"),
            chain_with_binding_input("chain:B", "binding-B", "Guitar"),
        ],
        midi: None,
    };

    let labels = input_port_labels_for_project(&project);

    assert_eq!(
        labels.len(),
        2,
        "two binding-input chains → 2 tuner rows; got {:?}",
        labels
    );
    assert!(
        labels[0] != labels[1],
        "rows must be labeled distinctly; both are {:?}",
        labels[0]
    );
}

/// Two bindings that share the same endpoint name must still appear as
/// separate rows — no deduplication across distinct binding ids.
#[test]
fn tuner_does_not_merge_same_endpoint_from_different_bindings() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            chain_with_binding_input("chain:A", "binding-A", "In1"),
            chain_with_binding_input("chain:B", "binding-B", "In1"),
        ],
        midi: None,
    };

    let labels = input_port_labels_for_project(&project);

    assert_eq!(
        labels.len(),
        2,
        "same endpoint name, different bindings → 2 rows; got {:?}",
        labels
    );
}

/// Legacy chains (io == "") must still enumerate as before — one row per
/// physical channel in the InputEntry list.
#[test]
fn tuner_legacy_entries_path_still_enumerates_per_channel() {
    let entries = vec![
        input_entry("dev:1", vec![0], ChainInputMode::Mono),
        input_entry("dev:1", vec![1], ChainInputMode::Mono),
    ];
    let project = project_from_chain(chain_with_input("chain:0", true, entries));

    let labels = input_port_labels_for_project(&project);

    assert_eq!(
        labels.len(),
        2,
        "two legacy entry channels → 2 tuner rows; got {:?}",
        labels
    );
}
