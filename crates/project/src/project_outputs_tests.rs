//! The physical output list every output picker shares.

use super::*;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::AudioDeviceDescriptor;

fn device(id: &str, name: &str) -> AudioDeviceDescriptor {
    AudioDeviceDescriptor {
        id: id.into(),
        name: name.into(),
        channels: 32,
    }
}

fn binding(id: &str, name: &str, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: name.into(),
        inputs: vec![],
        outputs,
    }
}

fn endpoint(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: domain::ids::DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

#[test]
fn output_endpoints_flattens_every_bindings_outputs() {
    let bindings = vec![
        binding(
            "main",
            "Scarlett 2i2",
            vec![endpoint("Main Out 1-2", "dev:scarlett", vec![0, 1])],
        ),
        binding(
            "monitor",
            "Headphones",
            vec![
                endpoint("Phones L", "dev:hp", vec![0]),
                endpoint("Phones R", "dev:hp", vec![1]),
            ],
        ),
    ];
    let devices = vec![
        device("dev:scarlett", "Scarlett 2i2"),
        device("dev:hp", "Headphones"),
    ];
    let outs = output_endpoints(&bindings, &devices);
    assert_eq!(
        outs.len(),
        3,
        "one entry per output endpoint across bindings"
    );
    assert_eq!(outs[0].label, "Scarlett 2i2 · Out 1/2");
    assert_eq!(outs[0].device_id, "dev:scarlett");
    assert_eq!(outs[0].channels, vec![0, 1]);
    assert_eq!(outs[2].label, "Headphones · Out 2");
    assert_eq!(outs[2].channels, vec![1]);
    // Keys are unique so the select can round-trip a pick.
    assert_ne!(outs[1].key, outs[2].key);
}

#[test]
fn resolve_output_endpoint_prefers_the_saved_one() {
    let bindings = vec![binding(
        "main",
        "Scarlett",
        vec![
            endpoint("Out A", "dev:x", vec![0, 1]),
            endpoint("Out B", "dev:x", vec![2, 3]),
        ],
    )];
    let outs = output_endpoints(&bindings, &[]);
    let saved = outs[1].key.clone();
    let picked = resolve_output_endpoint(Some(&saved), &outs).expect("saved endpoint resolves");
    assert_eq!(picked.channels, vec![2, 3], "the saved endpoint is chosen");
}

#[test]
fn resolve_output_endpoint_falls_back_to_the_first() {
    let bindings = vec![binding(
        "main",
        "Scarlett",
        vec![endpoint("Out A", "dev:x", vec![0, 1])],
    )];
    let outs = output_endpoints(&bindings, &[]);
    // A saved key from another machine / a renamed binding no longer resolves.
    let picked = resolve_output_endpoint(Some("gone::whatever"), &outs)
        .expect("falls back rather than going silent");
    assert_eq!(picked.channels, vec![0, 1]);
    // And with nothing saved.
    assert!(resolve_output_endpoint(None, &outs).is_some());
}

#[test]
fn resolve_output_endpoint_is_none_without_any_output() {
    assert!(resolve_output_endpoint(Some("main::x"), &[]).is_none());
    assert!(resolve_output_endpoint(None, &[]).is_none());
}

#[test]
fn output_endpoints_lists_each_physical_output_once() {
    // Several bindings routing to the same device channels are one output.
    let bindings = vec![
        binding(
            "g1",
            "Guitar 1",
            vec![
                endpoint("MAIN", "dev:hd8", vec![0, 1]),
                endpoint("FRFR", "dev:hd8", vec![24, 25]),
            ],
        ),
        binding(
            "g2",
            "Guitar 2",
            vec![
                endpoint("MAIN", "dev:hd8", vec![0, 1]),
                endpoint("FRFR", "dev:hd8", vec![24, 25]),
            ],
        ),
        binding(
            "dflt",
            "Default",
            vec![endpoint("Out1", "dev:hd8", vec![0, 1])],
        ),
    ];
    let outs = output_endpoints(&bindings, &[device("dev:hd8", "Quantum HD 8")]);
    assert_eq!(outs.len(), 2, "two physical outputs, not five endpoints");
    assert_eq!(
        outs[0].label, "Quantum HD 8 · Out 1/2",
        "device and channels, no binding name"
    );
    assert_eq!(outs[1].label, "Quantum HD 8 · Out 25/26");
    assert_eq!(outs[0].channels, vec![0, 1]);
    assert_eq!(outs[1].channels, vec![24, 25]);
}

#[test]
fn output_endpoints_keeps_the_same_channels_on_another_device() {
    let bindings = vec![
        binding("a", "A", vec![endpoint("Out", "dev:a", vec![0, 1])]),
        binding("b", "B", vec![endpoint("Out", "dev:b", vec![0, 1])]),
    ];
    assert_eq!(output_endpoints(&bindings, &[]).len(), 2);
}

#[test]
fn a_saved_key_of_a_merged_duplicate_still_resolves_to_its_output() {
    // A key saved before the merge points at an endpoint the list no longer
    // shows; it must land on the same channels, never on the first output.
    let bindings = vec![
        binding(
            "g1",
            "Guitar 1",
            vec![endpoint("SYN", "dev:hd8", vec![4, 5])],
        ),
        binding(
            "g2",
            "Guitar 2",
            vec![
                endpoint("MAIN", "dev:hd8", vec![0, 1]),
                endpoint("SYN", "dev:hd8", vec![4, 5]),
            ],
        ),
    ];
    let outs = output_endpoints(&bindings, &[]);
    let saved = endpoint_key("g2", "SYN");
    let picked = resolve_output_endpoint(Some(&saved), &outs).expect("resolves");
    assert_eq!(picked.channels, vec![4, 5]);
}

#[test]
fn an_output_on_a_device_the_host_no_longer_lists_is_named_by_its_id() {
    let bindings = vec![binding(
        "a",
        "A",
        vec![endpoint("Out", "dev:gone", vec![7])],
    )];
    let outs = output_endpoints(&bindings, &[]);
    assert_eq!(outs[0].label, "dev:gone · Out 8");
}

#[test]
fn each_output_carries_the_endpoint_that_names_it() {
    // DI and looper persist a (binding, endpoint) reference, so the list hands
    // back the canonical one for the row the user picked.
    let bindings = vec![
        binding(
            "g1",
            "Guitar 1",
            vec![endpoint("MAIN", "dev:hd8", vec![0, 1])],
        ),
        binding(
            "g2",
            "Guitar 2",
            vec![endpoint("FRFR", "dev:hd8", vec![24, 25])],
        ),
    ];
    let outs = output_endpoints(&bindings, &[]);
    assert_eq!(
        (outs[1].binding_id.as_str(), outs[1].endpoint.as_str()),
        ("g2", "FRFR")
    );
}

#[test]
fn output_position_finds_a_reference_through_its_alias() {
    let bindings = vec![
        binding(
            "g1",
            "Guitar 1",
            vec![endpoint("MAIN", "dev:hd8", vec![0, 1])],
        ),
        binding(
            "g2",
            "Guitar 2",
            vec![
                endpoint("FRFR", "dev:hd8", vec![24, 25]),
                endpoint("MAIN", "dev:hd8", vec![0, 1]),
            ],
        ),
    ];
    let outs = output_endpoints(&bindings, &[]);
    assert_eq!(output_position(&outs, "g2", "MAIN"), Some(0));
    assert_eq!(output_position(&outs, "g2", "FRFR"), Some(1));
    assert_eq!(output_position(&outs, "gone", "x"), None);
}
