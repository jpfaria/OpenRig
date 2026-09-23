use super::*;

use domain::ids::{BlockId, ChainId};
use domain::io_binding::{ChannelMode, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};

fn endpoint(name: &str, dev: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode,
        channels: channels.to_vec(),
    }
}

#[test]
fn the_tail_after_an_insert_is_fed_by_the_return_and_the_send_by_the_guitar() {
    let registry = vec![
        IoBinding {
            id: "g".into(),
            name: "G".into(),
            inputs: vec![endpoint("in", "a", ChannelMode::Mono, &[0])],
            outputs: vec![endpoint("main", "a", ChannelMode::Stereo, &[0, 1])],
        },
        IoBinding {
            id: "loop".into(),
            name: "LOOP".into(),
            inputs: vec![endpoint("ret", "b", ChannelMode::Stereo, &[0, 1])],
            outputs: vec![endpoint("snd", "b", ChannelMode::Mono, &[2])],
        },
    ];
    let chain = Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["g".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "loop".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    assert_eq!(
        route_producers(&chain, &registry),
        vec![vec![DeviceId("b".into())], vec![DeviceId("a".into())]]
    );
}
