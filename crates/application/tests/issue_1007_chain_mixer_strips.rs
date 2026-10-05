//! #1007: the compact view shows the mixer strips of ONE chain — only the
//! inputs and outputs that chain itself uses (its selected I/O bindings),
//! never the other endpoints of the machine.

use application::chain_mixer_strips::chain_mixer_strip_ids;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

fn endpoint(name: &str, dev: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode,
        channels,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitar-1".into(),
            name: "Guitar 1".into(),
            inputs: vec![endpoint("GUITAR 1", "hd8", ChannelMode::Mono, vec![0])],
            outputs: vec![endpoint("MAIN", "hd8", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "guitar-2".into(),
            name: "Guitar 2".into(),
            inputs: vec![endpoint("GUITAR 2", "hd8", ChannelMode::Mono, vec![1])],
            outputs: vec![endpoint("REAMP", "hd8", ChannelMode::Stereo, vec![10, 11])],
        },
        IoBinding {
            id: "syn".into(),
            name: "SYN".into(),
            inputs: vec![endpoint(
                "SYN L/R",
                "hd8",
                ChannelMode::Stereo,
                vec![14, 15],
            )],
            outputs: vec![endpoint("MAIN", "hd8", ChannelMode::Stereo, vec![0, 1])],
        },
    ]
}

fn chain(bindings: &[&str]) -> Chain {
    Chain {
        disabled_endpoints: Default::default(),
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks: Vec::new(),
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    }
}

#[test]
fn a_chain_lists_only_the_endpoints_of_its_own_binding() {
    let ids = chain_mixer_strip_ids(&chain(&["guitar-1"]), &registry());
    assert_eq!(ids, vec!["in:0@hd8".to_string(), "out:0,1@hd8".to_string()]);
}

#[test]
fn a_chain_on_two_bindings_lists_both_inputs_first_without_repeating_a_shared_output() {
    let ids = chain_mixer_strip_ids(&chain(&["guitar-1", "syn"]), &registry());
    assert_eq!(
        ids,
        vec![
            "in:0@hd8".to_string(),
            "in:14,15@hd8".to_string(),
            "out:0,1@hd8".to_string(),
        ]
    );
}

#[test]
fn a_chain_with_no_binding_has_no_strips() {
    assert!(chain_mixer_strip_ids(&chain(&[]), &registry()).is_empty());
}
