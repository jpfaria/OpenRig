use super::*;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::device::DeviceSettings;

/// #716: a tuner input's device/channels/mode come from the binding registry,
/// not from block `entries`. Each test input is one binding endpoint.
fn in_ep(device: &str, channels: Vec<usize>, mode: ChannelMode) -> IoEndpoint {
    IoEndpoint {
        name: "in0".into(),
        device_id: DeviceId(device.into()),
        mode,
        channels,
    }
}

/// One registry binding (`id`) carrying the given input endpoint(s).
fn binding(id: &str, inputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs,
        outputs: vec![],
    }
}

/// A binding-bound chain: it references `io1`; the registry holds the device.
fn chain_bound(id: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("Guitar".into()),
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec!["io1".into()],
        blocks: vec![],
        di_output: None,
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
    let registry = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0], ChannelMode::Mono)],
    )];
    let fp_enabled =
        project_input_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &registry);
    let fp_disabled = project_input_fingerprint(
        &project_from_chain(chain_bound("chain:0", false)),
        &registry,
    );

    assert_ne!(fp_enabled, fp_disabled);
    assert!(
        fp_disabled.is_empty(),
        "disabled chains contribute nothing to the fingerprint, got {:?}",
        fp_disabled
    );
}

#[test]
fn fingerprint_changes_when_channels_change() {
    let mono = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0], ChannelMode::Mono)],
    )];
    let stereo = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0, 1], ChannelMode::Stereo)],
    )];

    let fp_mono =
        project_input_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &mono);
    let fp_stereo =
        project_input_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &stereo);

    assert_ne!(fp_mono, fp_stereo);
}

#[test]
fn fingerprint_changes_when_device_id_changes() {
    let dev_a = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0], ChannelMode::Mono)],
    )];
    let dev_b = vec![binding(
        "io1",
        vec![in_ep("dev:2", vec![0], ChannelMode::Mono)],
    )];

    let fp_a = project_input_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &dev_a);
    let fp_b = project_input_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &dev_b);

    assert_ne!(fp_a, fp_b);
}

#[test]
fn fingerprint_stable_for_identical_projects() {
    let registry = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0, 1], ChannelMode::Stereo)],
    )];
    let mk = || project_from_chain(chain_bound("chain:0", true));

    assert_eq!(
        project_input_fingerprint(&mk(), &registry),
        project_input_fingerprint(&mk(), &registry)
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
