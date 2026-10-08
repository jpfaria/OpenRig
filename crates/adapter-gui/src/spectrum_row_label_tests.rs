//! A spectrum row names its stream by binding + direction + 1-based
//! channels on both sides (ui-rules §5), never by the raw device id.

use super::spectrum_row_label;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::stream_io_labels::{chain_stream_io_labels, StreamIoLabels};
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
fn label_names_the_binding_and_channels_of_each_side() {
    let io = StreamIoLabels {
        input: "GUITARRA 1".into(),
        output: "MAIN".into(),
        input_channels: "1".into(),
        output_channels: "1,2".into(),
        ..Default::default()
    };

    assert_eq!(
        spectrum_row_label("Digital", Some(&io), 0, "L"),
        "DIGITAL  ·  GUITARRA 1 IN 1  →  MAIN OUT 1,2  ·  L"
    );
}

#[test]
fn label_without_stream_io_falls_back_to_the_stream_number() {
    assert_eq!(
        spectrum_row_label("Digital", None, 1, "R"),
        "DIGITAL  ·  STREAM 2  ·  R"
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
    let chain = chain(vec!["g1", "g2"]);

    let labels: Vec<String> = chain_stream_io_labels(&chain, &registry)
        .iter()
        .enumerate()
        .map(|(i, io)| spectrum_row_label("Digital", Some(io), i, "L"))
        .collect();

    assert_eq!(
        labels,
        vec![
            "DIGITAL  ·  GUITARRA 1 IN 11  →  GUITARRA 1 OUT 1,2  ·  L".to_string(),
            "DIGITAL  ·  GUITARRA 2 IN 12  →  GUITARRA 2 OUT 17,18  ·  L".to_string(),
        ]
    );
}
