use super::*;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::device::DeviceSettings;

/// #716: a spectrum stream's device/channels/mode come from the binding
/// registry, not from block `entries`. Each test input is one binding endpoint.
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
    let registry = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0], ChannelMode::Mono)],
    )];
    let fp_enabled =
        project_stream_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &registry);
    let fp_disabled = project_stream_fingerprint(
        &project_from_chain(chain_bound("chain:0", false)),
        &registry,
    );
    assert_ne!(fp_enabled, fp_disabled);
    assert!(fp_disabled.is_empty());
}

#[test]
fn fingerprint_changes_when_input_mode_changes() {
    let mono = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0], ChannelMode::Mono)],
    )];
    let stereo = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0, 1], ChannelMode::Stereo)],
    )];

    let fp_mono =
        project_stream_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &mono);
    let fp_stereo =
        project_stream_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &stereo);
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

    let fp_a =
        project_stream_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &dev_a);
    let fp_b =
        project_stream_fingerprint(&project_from_chain(chain_bound("chain:0", true)), &dev_b);
    assert_ne!(fp_a, fp_b);
}

#[test]
fn fingerprint_stable_for_identical_projects() {
    let registry = vec![binding(
        "io1",
        vec![in_ep("dev:1", vec![0], ChannelMode::Mono)],
    )];
    let mk = || project_from_chain(chain_bound("chain:0", true));
    assert_eq!(
        project_stream_fingerprint(&mk(), &registry),
        project_stream_fingerprint(&mk(), &registry)
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
