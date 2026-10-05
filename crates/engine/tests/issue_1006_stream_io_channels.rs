//! #1006 — a meter row shows the binding name AND the channels it reads from
//! and writes to, so the input bar and the output bar never read the same.
//! Channels are 1-based, the way the interface labels them.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::stream_io_labels::chain_stream_io_labels;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

fn ep(mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: "ep".into(),
        device_id: DeviceId("coreaudio:quantum".into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn binding(id: &str, name: &str, input: &[usize], out: &[usize]) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: name.into(),
        inputs: vec![ep(ChannelMode::Mono, input)],
        outputs: vec![ep(ChannelMode::Stereo, out)],
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
        id: ChainId("rig:input-2".into()),
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
        binding("guitarra-1", "GUITARRA 1 - MAIN", &[0], &[0, 1]),
        binding("guitarra-1-5050", "GUITARRA 1 - SYN5050", &[0], &[16, 17]),
        binding("syn2-main", "SYN-2", &[17, 18], &[10]),
    ]
}

fn channels(chain: &Chain) -> Vec<(String, String)> {
    chain_stream_io_labels(chain, &registry())
        .into_iter()
        .map(|l| (l.input_channels, l.output_channels))
        .collect()
}

fn pair(i: &str, o: &str) -> (String, String) {
    (i.to_string(), o.to_string())
}

#[test]
fn each_row_carries_the_channels_of_its_binding() {
    let chain = chain(&["guitarra-1", "guitarra-1-5050"], vec![effect("gate")]);
    assert_eq!(
        channels(&chain),
        vec![pair("1", "1,2"), pair("1", "17,18")],
        "#1006: same input channel, each row its own output channels"
    );
}

#[test]
fn an_insert_row_carries_the_loop_channels() {
    let chain = chain(
        &["guitarra-1", "guitarra-1-5050"],
        vec![effect("gate"), insert("syn2", "syn2-main"), effect("delay")],
    );
    assert_eq!(
        channels(&chain),
        vec![
            pair("1", "11"),
            pair("1", "11"),
            pair("18,19", "1,2 + 17,18"),
        ],
        "#1006: head rows send to the loop, the return row feeds both tails"
    );
}
