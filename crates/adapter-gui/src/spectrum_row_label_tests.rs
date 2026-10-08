//! A spectrum row names its stream like the chain meters do: the interface
//! plus direction plus 1-based channels on both sides, never the raw device id.
//! A stream that feeds several outputs gets one pair of rows per output.

use super::{spectrum_output_labels, spectrum_row_label};
use crate::meter_row_labels::project_stream_labels;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::AudioDeviceDescriptor;
use engine::stream_io_labels::StreamIoLabels;
use project::chain::Chain;

const DEVICE: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8:1ed8:0210:0:QT9E25260495:11";

fn ep(name: &str, channels: Vec<usize>, mode: ChannelMode) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode,
        channels,
    }
}

fn chain(binding_ids: Vec<&str>) -> Chain {
    Chain {
        id: ChainId("chain:0".into()),
        description: Some("Digital".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: binding_ids.into_iter().map(String::from).collect(),
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

#[test]
fn label_names_the_interface_and_channels_of_each_side() {
    let io = StreamIoLabels {
        input: "Quantum HD 8".into(),
        output: "Quantum HD 8".into(),
        input_channels: "1".into(),
        output_channels: "1,2".into(),
        ..Default::default()
    };

    assert_eq!(
        spectrum_output_labels("Digital", Some(&io), 0),
        vec!["DIGITAL  ·  QUANTUM HD 8 IN 1  →  QUANTUM HD 8 OUT 1,2".to_string()]
    );
}

#[test]
fn label_without_a_known_interface_keeps_direction_and_channels() {
    let io = StreamIoLabels {
        input_channels: "3".into(),
        output_channels: "1,2".into(),
        ..Default::default()
    };

    assert_eq!(
        spectrum_output_labels("Digital", Some(&io), 0),
        vec!["DIGITAL  ·  IN 3  →  OUT 1,2".to_string()]
    );
}

#[test]
fn label_without_stream_io_falls_back_to_the_stream_number() {
    assert_eq!(
        spectrum_output_labels("Digital", None, 1),
        vec!["DIGITAL  ·  STREAM 2".to_string()]
    );
}

#[test]
fn streams_on_one_interface_get_distinct_labels_without_the_device_id() {
    let registry = vec![
        IoBinding {
            id: "g1".into(),
            name: "GUITARRA 1".into(),
            inputs: vec![ep("in", vec![10], ChannelMode::Mono)],
            outputs: vec![ep("out", vec![0, 1], ChannelMode::Stereo)],
        },
        IoBinding {
            id: "g2".into(),
            name: "GUITARRA 2".into(),
            inputs: vec![ep("in", vec![11], ChannelMode::Mono)],
            outputs: vec![ep("out", vec![16, 17], ChannelMode::Stereo)],
        },
    ];
    let devices = vec![AudioDeviceDescriptor {
        id: DEVICE.into(),
        name: "Quantum HD 8".into(),
        channels: 30,
    }];

    let labels: Vec<String> = project_stream_labels(&chain(vec!["g1", "g2"]), &registry, &devices)
        .iter()
        .enumerate()
        .flat_map(|(i, io)| spectrum_output_labels("Digital", Some(io), i))
        .map(|base| spectrum_row_label(&base, "L"))
        .collect();

    assert_eq!(
        labels,
        vec![
            "DIGITAL  ·  QUANTUM HD 8 IN 11  →  QUANTUM HD 8 OUT 1,2  ·  L".to_string(),
            "DIGITAL  ·  QUANTUM HD 8 IN 12  →  QUANTUM HD 8 OUT 17,18  ·  L".to_string(),
        ]
    );
}

#[test]
fn a_stream_feeding_several_outputs_gets_one_label_per_output() {
    let registry = vec![IoBinding {
        id: "g1".into(),
        name: "GUITARRA 1".into(),
        inputs: vec![ep("in", vec![0], ChannelMode::Mono)],
        outputs: vec![
            ep("main", vec![0, 1], ChannelMode::Stereo),
            ep("frfr", vec![24, 25], ChannelMode::Stereo),
            ep("syn", vec![4, 5], ChannelMode::Stereo),
        ],
    }];
    let devices = vec![AudioDeviceDescriptor {
        id: DEVICE.into(),
        name: "Quantum HD 8".into(),
        channels: 30,
    }];

    let labels: Vec<String> = project_stream_labels(&chain(vec!["g1"]), &registry, &devices)
        .iter()
        .enumerate()
        .flat_map(|(i, io)| spectrum_output_labels("Digital", Some(io), i))
        .collect();

    assert_eq!(
        labels,
        vec![
            "DIGITAL  ·  QUANTUM HD 8 IN 1  →  QUANTUM HD 8 OUT 1,2".to_string(),
            "DIGITAL  ·  QUANTUM HD 8 IN 1  →  QUANTUM HD 8 OUT 25,26".to_string(),
            "DIGITAL  ·  QUANTUM HD 8 IN 1  →  QUANTUM HD 8 OUT 5,6".to_string(),
        ]
    );
}

#[test]
fn row_label_appends_the_side() {
    assert_eq!(
        spectrum_row_label("DIGITAL  ·  IN 1  →  OUT 1,2", "R"),
        "DIGITAL  ·  IN 1  →  OUT 1,2  ·  R"
    );
}
