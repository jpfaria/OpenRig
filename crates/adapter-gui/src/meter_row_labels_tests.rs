//! A meter row names the audio interface, as the host names it — never the
//! I/O binding: "QUANTUM HD 8  IN 1", "QUANTUM HD 8  OUT 1,2".

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::AudioDeviceDescriptor;
use project::chain::Chain;

use super::project_stream_labels;
use crate::meter_wiring::rebuild_stream_meters_row;

const QUANTUM: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8:1ed8";
const AMPERO: &str = "coreaudio:ampero";

fn ep(device: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: "ep".into(),
        device_id: DeviceId(device.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "GUITARRA 1 - MAIN + FRFR".into(),
            inputs: vec![ep(QUANTUM, ChannelMode::Mono, &[0])],
            outputs: vec![ep(QUANTUM, ChannelMode::Stereo, &[0, 1])],
        },
        IoBinding {
            id: "ampero".into(),
            name: "GUITARRA 1 - AMPERO".into(),
            inputs: vec![ep(QUANTUM, ChannelMode::Mono, &[0])],
            outputs: vec![ep(AMPERO, ChannelMode::Stereo, &[2, 3])],
        },
    ]
}

fn chain(bindings: &[&str]) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: Some("GUITARRA 1".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn devices() -> Vec<AudioDeviceDescriptor> {
    vec![
        AudioDeviceDescriptor {
            id: QUANTUM.into(),
            name: "Quantum HD 8".into(),
            channels: 30,
        },
        AudioDeviceDescriptor {
            id: AMPERO.into(),
            name: "Ampero II".into(),
            channels: 4,
        },
    ]
}

#[test]
fn rows_name_the_interface_not_the_binding() {
    let labels = project_stream_labels(&chain(&["main"]), &registry(), &devices());
    let rows = rebuild_stream_meters_row(&[], 1, &labels, 100.0, true);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].in_label.as_str(), "Quantum HD 8");
    assert_eq!(rows[0].in_channels.as_str(), "1");
    assert_eq!(rows[0].out_label.as_str(), "Quantum HD 8");
    assert_eq!(rows[0].out_channels.as_str(), "1,2");
}

#[test]
fn each_output_row_names_its_own_interface() {
    let labels = project_stream_labels(&chain(&["main", "ampero"]), &registry(), &devices());
    let rows = rebuild_stream_meters_row(&[], labels.len(), &labels, 100.0, true);

    let outs: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.out_label.to_string(), r.out_channels.to_string()))
        .collect();
    assert_eq!(
        outs,
        vec![
            ("Quantum HD 8".to_string(), "1,2".to_string()),
            ("Ampero II".to_string(), "3,4".to_string()),
        ]
    );
    let names: Vec<&str> = labels.iter().map(|l| l.output.as_str()).collect();
    assert_eq!(names, vec!["Quantum HD 8", "Ampero II"]);
}

#[test]
fn an_interface_the_host_has_not_listed_leaves_the_name_blank() {
    // Before the first device enumeration lands, the row shows only its
    // direction and channels — never the binding name, never the raw id.
    let labels = project_stream_labels(&chain(&["main"]), &registry(), &[]);
    let rows = rebuild_stream_meters_row(&[], 1, &labels, 100.0, true);

    assert_eq!(rows[0].in_label.as_str(), "");
    assert_eq!(rows[0].out_label.as_str(), "");
    assert_eq!(rows[0].in_channels.as_str(), "1");
}
