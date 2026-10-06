//! A meter row names the audio interface it reads from and writes to,
//! not the I/O binding. The labels carry the device of the stream's input and
//! of every output, so the GUI can print the interface's host name.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::stream_io_labels::chain_stream_io_labels;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const QUANTUM: &str = "coreaudio:quantum";
const AMPERO: &str = "coreaudio:ampero";

fn ep(device: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: "ep".into(),
        device_id: DeviceId(device.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn binding(id: &str, input: (&str, &[usize]), out: (&str, &[usize])) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: format!("BINDING {id}"),
        inputs: vec![ep(input.0, ChannelMode::Mono, input.1)],
        outputs: vec![ep(out.0, ChannelMode::Stereo, out.1)],
    }
}

fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: domain::ids::BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn insert(id: &str, io: &str) -> AudioBlock {
    AudioBlock {
        id: domain::ids::BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    }
}

fn chain(bindings: &[&str], blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: Some("GUITARRA 1".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        binding("main", (QUANTUM, &[0]), (QUANTUM, &[0, 1])),
        binding("ampero", (QUANTUM, &[0]), (AMPERO, &[2, 3])),
        binding("pedais", (QUANTUM, &[25]), (QUANTUM, &[7])),
    ]
}

fn output_devices(labels: &engine::stream_io_labels::StreamIoLabels) -> Vec<String> {
    labels.outputs.iter().map(|o| o.device.clone()).collect()
}

#[test]
fn a_row_carries_the_device_of_its_input_and_of_each_output() {
    let labels = chain_stream_io_labels(&chain(&["main", "ampero"], vec![]), &registry());

    assert!(
        labels.iter().all(|row| row.input_device == QUANTUM),
        "every row reads the jack's interface"
    );
    let outputs: Vec<String> = labels.iter().flat_map(output_devices).collect();
    assert_eq!(
        outputs,
        vec![QUANTUM.to_string(), AMPERO.to_string()],
        "every output names the interface it writes to"
    );
}

#[test]
fn an_insert_row_carries_the_loop_device() {
    let labels = chain_stream_io_labels(
        &chain(
            &["main"],
            vec![effect("gate"), insert("loop", "pedais"), effect("delay")],
        ),
        &registry(),
    );

    assert_eq!(labels.len(), 2, "head row + return row");
    assert_eq!(labels[0].input_device, QUANTUM);
    assert_eq!(output_devices(&labels[0]), vec![QUANTUM.to_string()]);
    assert_eq!(
        labels[0].outputs[0].channels, "8",
        "the send goes out OUT 8"
    );
    assert_eq!(labels[1].input_device, QUANTUM);
    assert_eq!(labels[1].input_channels, "26", "the return comes in IN 26");
}
