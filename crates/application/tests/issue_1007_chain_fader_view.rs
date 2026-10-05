//! #1007: the compact view reads a chain's OWN fader on each strip it plays
//! through — the value stored in the project, unity when nobody moved it.

use application::chain_fader_view::{chain_fader_views, ChainFaderView};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::mixer_strip::MixerDirection;
use project::chain::Chain;

fn endpoint(name: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("hd8".into()),
        mode,
        channels,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitar-1".into(),
            name: "Guitar 1".into(),
            inputs: vec![endpoint("GUITAR 1", ChannelMode::Mono, vec![0])],
            outputs: vec![endpoint("MAIN", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "syn".into(),
            name: "SYN".into(),
            inputs: vec![endpoint("SYN L/R", ChannelMode::Stereo, vec![14, 15])],
            outputs: vec![endpoint("MAIN", ChannelMode::Stereo, vec![0, 1])],
        },
    ]
}

fn chain() -> Chain {
    Chain {
        disabled_endpoints: Default::default(),
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitar-1".into(), "syn".into()],
        blocks: Vec::new(),
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    }
}

fn view(strip: &str, gain_db: f32, muted: bool) -> ChainFaderView {
    ChainFaderView {
        strip: strip.into(),
        gain_db,
        muted,
    }
}

#[test]
fn an_untouched_chain_reads_unity_on_every_strip_once() {
    assert_eq!(
        chain_fader_views(&chain(), &registry()),
        vec![
            view("in:0@hd8", 0.0, false),
            view("in:14,15@hd8", 0.0, false),
            view("out:0,1@hd8", 0.0, false),
        ]
    );
}

#[test]
fn a_moved_fader_is_read_from_the_first_port_of_its_strip() {
    let mut chain = chain();
    chain
        .mix
        .endpoint_mut(MixerDirection::Output, "guitar-1", "MAIN")
        .gain_db = -6.0;
    chain
        .mix
        .endpoint_mut(MixerDirection::Input, "syn", "SYN L/R")
        .muted = true;
    assert_eq!(
        chain_fader_views(&chain, &registry()),
        vec![
            view("in:0@hd8", 0.0, false),
            view("in:14,15@hd8", 0.0, true),
            view("out:0,1@hd8", -6.0, false),
        ]
    );
}
