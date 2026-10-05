//! Which row of the project's output list a chain's DI / looper picker shows.

use super::*;
use crate::project_outputs::output_endpoints;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoEndpoint};

fn out(name: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "other".into(),
            name: "Other".into(),
            inputs: vec![],
            outputs: vec![out("FRFR", vec![24, 25])],
        },
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![],
            outputs: vec![out("Main", vec![0, 1]), out("FX", vec![2, 3])],
        },
    ]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

#[test]
fn nothing_saved_shows_the_chains_main_output() {
    let outputs = output_endpoints(&registry(), &[]);
    assert_eq!(chain_output_index(&chain(), &registry(), &outputs, None), 1);
}

#[test]
fn a_saved_output_outside_the_chain_is_shown() {
    let outputs = output_endpoints(&registry(), &[]);
    let saved = Some(("other", "FRFR"));
    assert_eq!(
        chain_output_index(&chain(), &registry(), &outputs, saved),
        0
    );
}

#[test]
fn a_stale_reference_shows_the_chains_main_output() {
    let outputs = output_endpoints(&registry(), &[]);
    let saved = Some(("gone", "x"));
    assert_eq!(
        chain_output_index(&chain(), &registry(), &outputs, saved),
        1
    );
}

#[test]
fn a_chain_without_outputs_and_nothing_saved_shows_no_row() {
    let mut c = chain();
    c.io_binding_ids.clear();
    let outputs = output_endpoints(&registry(), &[]);
    assert_eq!(chain_output_index(&c, &registry(), &outputs, None), -1);
}
