//! A physical endpoint declared by several bindings is listed once.

use super::*;
use crate::io_binding::ChannelMode;

fn ep(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

fn binding(id: &str, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![],
        outputs,
    }
}

fn outputs(bindings: &[IoBinding]) -> Vec<DistinctEndpoint> {
    distinct_endpoints(
        bindings
            .iter()
            .flat_map(|b| b.outputs.iter().map(move |e| (b, e))),
    )
}

#[test]
fn the_same_device_and_channels_is_one_entry_with_every_copy_as_alias() {
    let bindings = vec![
        binding("g1", vec![ep("MAIN", "hd8", vec![0, 1])]),
        binding(
            "g2",
            vec![
                ep("MAIN", "hd8", vec![0, 1]),
                ep("FRFR", "hd8", vec![14, 15]),
            ],
        ),
    ];
    let list = outputs(&bindings);
    let labels: Vec<&str> = list.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(labels, vec!["MAIN", "FRFR"]);
    assert_eq!(list[0].binding_id, "g1");
    assert!(list[0].carries("g2", "MAIN"));
    assert_eq!(position_of(&list, "g2", "MAIN"), Some(0));
    assert_eq!(position_of(&list, "g2", "FRFR"), Some(1));
    assert_eq!(position_of(&list, "gone", "MAIN"), None);
}

#[test]
fn a_name_shared_by_different_endpoints_carries_the_binding_name() {
    let bindings = vec![
        binding("a", vec![ep("Out", "dev-a", vec![0, 1])]),
        binding("b", vec![ep("Out", "dev-b", vec![0, 1])]),
    ];
    let labels: Vec<String> = outputs(&bindings).into_iter().map(|d| d.label).collect();
    assert_eq!(labels, vec!["A · Out", "B · Out"]);
}
