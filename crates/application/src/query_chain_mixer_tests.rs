//! #1007: `openrig://chains/{chain}/mixer` — read parity for the chain
//! mixer commands.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::mixer_strip::MixerDirection;
use project::chain::Chain;
use project::project::Project;

use super::chain_mixer_json;

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar".into(),
            device_id: DeviceId("hd8".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Main".into(),
            device_id: DeviceId("hd8".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn project() -> Project {
    let mut chain = Chain {
        disabled_endpoints: Default::default(),
        id: ChainId("rig:guitar".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    };
    chain.mix.di_gain_db = -3.0;
    let out = chain.mix.endpoint_mut(MixerDirection::Output, "io", "Main");
    out.gain_db = -6.0;
    out.muted = true;
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    }
}

#[test]
fn reads_the_chain_faders_and_its_di_fader() {
    let json = chain_mixer_json(&project(), &registry(), &ChainId("rig:guitar".into()))
        .expect("the chain exists");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["chain"], "rig:guitar");
    assert_eq!(v["di_gain_db"], -3.0);
    assert_eq!(v["strips"][0]["id"], "in:0@hd8");
    assert_eq!(v["strips"][0]["gain_db"], 0.0);
    assert_eq!(v["strips"][0]["muted"], false);
    assert_eq!(v["strips"][1]["id"], "out:0,1@hd8");
    assert_eq!(v["strips"][1]["gain_db"], -6.0);
    assert_eq!(v["strips"][1]["muted"], true);
}

#[test]
fn an_unknown_chain_is_an_error() {
    assert!(chain_mixer_json(&project(), &registry(), &ChainId("nope".into())).is_err());
}
