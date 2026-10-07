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
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
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

/// A subscription that carries nothing — enough to build a row.
struct EmptyTap;

impl application::audio_taps::AudioTap for EmptyTap {
    fn channels(&self) -> usize {
        1
    }
    fn poll_peak_dbfs(&self) -> f32 {
        engine::output_meter::SILENT_DBFS
    }
}

#[test]
fn row_state_starts_with_empty_buffer() {
    let state = RowState::new(Arc::new(EmptyTap), 0, 48_000, DEFAULT_REFERENCE_HZ);
    assert!(state.sample_buf.is_empty());
    assert!(state.sample_buf.capacity() >= BUFFER_SIZE * 2);
}

// ── one row per physical input (#398) ────────────────────────────────────

const SR: u32 = 48_000;

/// A guitar plugged into a channel: every drain hands back the next stretch
/// of a sine at `hz`.
struct SineTap {
    hz: f32,
    channels: usize,
    at: std::sync::Mutex<usize>,
}

impl application::audio_taps::AudioTap for SineTap {
    fn channels(&self) -> usize {
        self.channels
    }
    fn poll_peak_dbfs(&self) -> f32 {
        -12.0
    }
    fn drain_channel(&self, _channel: usize, _max: usize, out: &mut Vec<f32>) -> usize {
        let mut at = self.at.lock().unwrap();
        let n = BUFFER_SIZE * 2;
        for i in 0..n {
            let t = (*at + i) as f32 / SR as f32;
            out.push(0.2 * (std::f32::consts::TAU * self.hz * t).sin());
        }
        *at += n;
        n
    }
}

/// Every input of every chain carries the same string at `hz`.
struct Strings {
    hz: f32,
}

impl AudioTaps for Strings {
    fn is_hosted(&self) -> bool {
        true
    }
    fn live_sample_rate(&self) -> u32 {
        SR
    }
    fn subscribe(&self, point: &TapPoint, _capacity: usize) -> Option<Arc<dyn AudioTap>> {
        match point {
            TapPoint::InputChannels { channels, .. } => Some(Arc::new(SineTap {
                hz: self.hz,
                channels: channels.len(),
                at: std::sync::Mutex::new(0),
            })),
            _ => None,
        }
    }
}

fn named_in(name: &str, device: &str, channel: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![channel],
    }
}

fn chain_with(id: &str, bindings: &[&str]) -> Chain {
    Chain {
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        ..chain_bound(id, true)
    }
}

fn project_with(chains: Vec<Chain>) -> Project {
    Project {
        chains,
        ..project_from_chain(chain_bound("unused", false))
    }
}

/// The owner's ANAL+DIG chain: the same guitar reaches it through the
/// amp binding and the SYN-5050 binding.
fn two_bindings_one_guitar() -> Vec<IoBinding> {
    vec![
        binding("guitarra-1", vec![named_in("GUITARRA 1", "hd8", 0)]),
        binding("guitarra-1-syn5050", vec![named_in("GUITARRA 1", "hd8", 0)]),
        binding("guitarra-2", vec![named_in("GUITARRA 2", "hd8", 1)]),
        binding("guitarra-2-syn5050", vec![named_in("GUITARRA 2", "hd8", 1)]),
    ]
}

/// The interfaces the host lists.
fn host() -> Vec<domain::AudioDeviceDescriptor> {
    vec![
        domain::AudioDeviceDescriptor {
            id: "hd8".into(),
            name: "Quantum HD 8".into(),
            channels: 30,
        },
        domain::AudioDeviceDescriptor {
            id: "scarlett".into(),
            name: "Scarlett 2i2".into(),
            channels: 2,
        },
    ]
}

fn labels(session: &TunerSession) -> Vec<String> {
    session
        .rows_model
        .iter()
        .map(|r| r.label.to_string())
        .collect()
}

#[test]
fn a_guitar_two_bindings_reach_is_tuned_once() {
    let project = project_with(vec![chain_with(
        "rig:input-7",
        &[
            "guitarra-1",
            "guitarra-2",
            "guitarra-1-syn5050",
            "guitarra-2-syn5050",
        ],
    )]);
    let session = TunerSession::build(
        &project,
        &Strings { hz: 82.41 },
        &two_bindings_one_guitar(),
        &host(),
    );
    assert_eq!(
        labels(&session),
        vec!["Quantum HD 8 · IN 1", "Quantum HD 8 · IN 2"],
        "two guitars on two channels are two rows, whatever binds them"
    );
}

#[test]
fn the_same_guitar_in_two_chains_is_tuned_once() {
    let project = project_with(vec![
        chain_with("rig:a", &["guitarra-1"]),
        chain_with("rig:b", &["guitarra-1-syn5050"]),
    ]);
    let session = TunerSession::build(
        &project,
        &Strings { hz: 82.41 },
        &two_bindings_one_guitar(),
        &host(),
    );
    assert_eq!(labels(&session), vec!["Quantum HD 8 · IN 1"]);
}

#[test]
fn the_same_channel_on_another_interface_is_another_guitar() {
    let registry = vec![
        binding("a", vec![named_in("GUITARRA 1", "hd8", 0)]),
        binding("b", vec![named_in("BAIXO", "scarlett", 0)]),
    ];
    let project = project_with(vec![chain_with("rig:a", &["a", "b"])]);
    let session = TunerSession::build(&project, &Strings { hz: 82.41 }, &registry, &host());
    assert_eq!(
        labels(&session),
        vec!["Quantum HD 8 · IN 1", "Scarlett 2i2 · IN 1"]
    );
}

#[test]
fn a_row_is_named_after_the_interface_and_the_input_on_its_panel() {
    let registry = vec![binding("syn", vec![named_in("SYN5050", "hd8", 4)])];
    let project = project_with(vec![chain_with("rig:a", &["syn"])]);
    let session = TunerSession::build(&project, &Strings { hz: 82.41 }, &registry, &host());
    assert_eq!(labels(&session), vec!["Quantum HD 8 · IN 5"]);
}

#[test]
fn an_interface_the_host_no_longer_lists_is_named_after_the_input() {
    let registry = vec![binding("syn", vec![named_in("SYN5050", "gone", 4)])];
    let project = project_with(vec![chain_with("rig:a", &["syn"])]);
    let session = TunerSession::build(&project, &Strings { hz: 82.41 }, &registry, &host());
    assert_eq!(labels(&session), vec!["SYN5050 · IN 5"]);
}

#[test]
fn a_stereo_input_names_each_of_its_channels() {
    let registry = vec![binding(
        "keys",
        vec![IoEndpoint {
            name: "KEYS".into(),
            device_id: DeviceId("hd8".into()),
            mode: ChannelMode::Stereo,
            channels: vec![2, 3],
        }],
    )];
    let project = project_with(vec![chain_with("rig:a", &["keys"])]);
    let session = TunerSession::build(&project, &Strings { hz: 82.41 }, &registry, &host());
    assert_eq!(
        labels(&session),
        vec!["Quantum HD 8 · IN 3", "Quantum HD 8 · IN 4"]
    );
}

// ── in tune (#398) ───────────────────────────────────────────────────────

/// E2 detuned by `cents`.
fn e2(cents: f32) -> f32 {
    let e2 = DEFAULT_REFERENCE_HZ * 2f32.powf((20.0 - 49.0) / 12.0);
    e2 * 2f32.powf(cents / 1200.0)
}

fn played(hz: f32) -> TunerRow {
    let registry = vec![binding("g", vec![named_in("GUITARRA 1", "hd8", 0)])];
    let project = project_with(vec![chain_with("rig:a", &["g"])]);
    let mut session = TunerSession::build(&project, &Strings { hz }, &registry, &host());
    for _ in 0..12 {
        session.tick();
    }
    session.rows_model.row_data(0).unwrap()
}

#[test]
fn a_string_within_the_tolerance_reads_in_tune() {
    let row = played(e2(1.0));
    assert!(row.active, "the string is heard");
    assert_eq!(row.note, "E");
    assert!(row.in_tune, "{} ct is in tune", row.cents);
}

#[test]
fn a_string_ten_cents_off_is_not_in_tune() {
    let row = played(e2(10.0));
    assert!(row.active, "the string is heard");
    assert!(!row.in_tune, "{} ct is not in tune", row.cents);
}

#[test]
fn a_silent_input_is_never_in_tune() {
    let row = placeholder_row("GUITARRA 1 · IN 1".into());
    assert!(!row.in_tune);
}
