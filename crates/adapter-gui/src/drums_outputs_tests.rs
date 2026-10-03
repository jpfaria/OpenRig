use domain::ids::DeviceId;
use infra_filesystem::{ChannelMode, IoBinding, IoEndpoint};

use super::drums_output_endpoints;

fn endpoint(name: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

fn binding(id: &str, inputs: usize, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: (0..inputs)
            .map(|i| endpoint(&format!("In {i}"), vec![i]))
            .collect(),
        outputs,
    }
}

#[test]
fn only_output_only_bindings_are_offered() {
    let bindings = vec![
        binding("guitar-1", 1, vec![endpoint("MAIN", vec![0, 1])]),
        binding("guitar-2", 1, vec![endpoint("MAIN", vec![0, 1])]),
        binding(
            "main-frfr",
            0,
            vec![endpoint("MAIN", vec![0, 1]), endpoint("FRFR", vec![24, 25])],
        ),
        binding("syn5050", 0, vec![endpoint("SYN-5050", vec![4, 5])]),
    ];
    let labels: Vec<String> = drums_output_endpoints(&bindings)
        .into_iter()
        .map(|o| o.label)
        .collect();
    assert_eq!(
        labels,
        vec![
            "MAIN-FRFR · MAIN".to_string(),
            "MAIN-FRFR · FRFR".to_string(),
            "SYN5050 · SYN-5050".to_string(),
        ]
    );
}

#[test]
fn with_no_output_only_binding_every_output_is_offered() {
    // A fresh install has one in+out binding; the drums must still sound.
    let bindings = vec![binding("default", 1, vec![endpoint("Out1", vec![0, 1])])];
    let labels: Vec<String> = drums_output_endpoints(&bindings)
        .into_iter()
        .map(|o| o.label)
        .collect();
    assert_eq!(labels, vec!["DEFAULT · Out1".to_string()]);
}
