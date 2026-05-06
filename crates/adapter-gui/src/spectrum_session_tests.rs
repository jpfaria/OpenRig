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
        }),
    }
}

fn chain_with_input(id: &str, enabled: bool, entries: Vec<InputEntry>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("Guitar".into()),
        instrument: "electric_guitar".to_string(),
        enabled,
        blocks: vec![input_block(entries)],
    }
}

fn project_from_chain(chain: Chain) -> Project {
    Project {
        name: None,
        device_settings: Vec::<DeviceSettings>::new(),
        chains: vec![chain],
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
