//! #1007: a chain's own mixer faders — stored in the project, pushed to the
//! engine's chain-local scalars, never touching the global strip or another
//! chain.
//!
//! The engine tables are process-global, so every test uses its OWN device
//! id and chain ids.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::mixer_gain::strip_linear_gain;
use domain::mixer_strip::MixerDirection;
use engine::chain_mix_gains::{chain_di_gain_target, chain_endpoint_gain_target};
use engine::mixer_gains::endpoint_gain_target;
use project::chain::Chain;
use project::project::Project;

use crate::command::{Command, MixerCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn chain(id: &str) -> Chain {
    Chain {
        disabled_endpoints: Default::default(),
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    }
}

/// Two chains through the same binding on `device`: a mono guitar pair
/// (channels 0 and 1) in, a stereo main out.
fn rig(device: &str, a: &str, b: &str) -> LocalDispatcher {
    let d = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain(a), chain(b)],
        midi: None,
    })));
    d.attach_io_bindings(Rc::new(RefCell::new(vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar".into(),
            device_id: DeviceId(device.into()),
            mode: ChannelMode::Mono,
            channels: vec![0, 1],
        }],
        outputs: vec![IoEndpoint {
            name: "Main".into(),
            device_id: DeviceId(device.into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }])));
    d
}

fn run(d: &LocalDispatcher, cmd: MixerCommand) -> Vec<Event> {
    d.dispatch(Command::Mixer(cmd))
        .expect("chain mixer command dispatches")
}

fn target(chain: &str, dir: MixerDirection, device: &str, channels: &[usize]) -> f32 {
    chain_endpoint_gain_target(&ChainId(chain.into()), dir, device, channels)
}

fn mix_of(d: &LocalDispatcher, id: &str) -> project::chain::ChainMix {
    d.project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == id)
        .expect("chain exists")
        .mix
        .clone()
}

#[test]
fn a_chain_fader_moves_only_that_chain_on_that_endpoint() {
    let dev = "cmx-fader";
    let d = rig(dev, "cmx-fader-a", "cmx-fader-b");
    let events = run(
        &d,
        MixerCommand::SetChainMixerFader {
            chain: ChainId("cmx-fader-a".into()),
            strip: format!("out:0,1@{dev}"),
            gain_db: -6.0,
        },
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::ChainMixerStripChanged { chain, strip, gain_db, muted: false }
            if chain.0 == "cmx-fader-a" && strip == &format!("out:0,1@{dev}") && *gain_db == -6.0
    )));
    assert!(events.iter().any(|e| matches!(e, Event::ProjectMutated)));
    let expected = strip_linear_gain(-6.0, false);
    assert_eq!(
        target("cmx-fader-a", MixerDirection::Output, dev, &[0, 1]),
        expected
    );
    assert_eq!(
        target("cmx-fader-b", MixerDirection::Output, dev, &[0, 1]),
        1.0
    );
    assert_eq!(
        endpoint_gain_target(MixerDirection::Output, dev, &[0, 1]),
        1.0
    );
    let mix = mix_of(&d, "cmx-fader-a");
    let fader = mix
        .endpoint(MixerDirection::Output, "io", "Main")
        .expect("the moved fader is stored in the project");
    assert_eq!(fader.gain_db, -6.0);
    assert!(mix_of(&d, "cmx-fader-b").is_unity());
}

#[test]
fn a_mono_input_strip_reaches_every_channel_pipeline_of_the_chain() {
    let dev = "cmx-mono";
    let d = rig(dev, "cmx-mono-a", "cmx-mono-b");
    run(
        &d,
        MixerCommand::SetChainMixerFader {
            chain: ChainId("cmx-mono-a".into()),
            strip: format!("in:0,1@{dev}"),
            gain_db: -12.0,
        },
    );
    let expected = strip_linear_gain(-12.0, false);
    assert_eq!(
        target("cmx-mono-a", MixerDirection::Input, dev, &[0]),
        expected
    );
    assert_eq!(
        target("cmx-mono-a", MixerDirection::Input, dev, &[1]),
        expected
    );
    assert_eq!(target("cmx-mono-b", MixerDirection::Input, dev, &[0]), 1.0);
}

#[test]
fn the_chain_fader_is_clamped() {
    let dev = "cmx-clamp";
    let d = rig(dev, "cmx-clamp-a", "cmx-clamp-b");
    run(
        &d,
        MixerCommand::SetChainMixerFader {
            chain: ChainId("cmx-clamp-a".into()),
            strip: format!("out:0,1@{dev}"),
            gain_db: 40.0,
        },
    );
    let mix = mix_of(&d, "cmx-clamp-a");
    assert_eq!(
        mix.endpoint(MixerDirection::Output, "io", "Main")
            .expect("stored")
            .gain_db,
        12.0
    );
}

#[test]
fn mute_silences_the_chain_and_unmute_at_unity_leaves_the_project_clean() {
    let dev = "cmx-mute";
    let d = rig(dev, "cmx-mute-a", "cmx-mute-b");
    let strip = format!("out:0,1@{dev}");
    let a = ChainId("cmx-mute-a".into());
    run(
        &d,
        MixerCommand::ToggleChainMixerMute {
            chain: a.clone(),
            strip: strip.clone(),
        },
    );
    assert_eq!(
        target("cmx-mute-a", MixerDirection::Output, dev, &[0, 1]),
        0.0
    );
    assert_eq!(
        target("cmx-mute-b", MixerDirection::Output, dev, &[0, 1]),
        1.0
    );
    run(
        &d,
        MixerCommand::SetChainMixerMute {
            chain: a,
            strip,
            muted: false,
        },
    );
    assert_eq!(
        target("cmx-mute-a", MixerDirection::Output, dev, &[0, 1]),
        1.0
    );
    assert!(mix_of(&d, "cmx-mute-a").is_unity());
}

#[test]
fn a_strip_the_chain_does_not_play_through_is_refused() {
    let dev = "cmx-foreign";
    let d = rig(dev, "cmx-foreign-a", "cmx-foreign-b");
    let result = d.dispatch(Command::Mixer(MixerCommand::SetChainMixerFader {
        chain: ChainId("cmx-foreign-a".into()),
        strip: "out:4,5@some-other-device".into(),
        gain_db: -3.0,
    }));
    assert!(result.is_err());
    assert!(mix_of(&d, "cmx-foreign-a").is_unity());
}

#[test]
fn the_di_fader_moves_only_that_chain() {
    let dev = "cmx-di";
    let d = rig(dev, "cmx-di-a", "cmx-di-b");
    let events = run(
        &d,
        MixerCommand::SetChainDiFader {
            chain: ChainId("cmx-di-a".into()),
            gain_db: -80.0,
        },
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::ChainDiFaderChanged { chain, gain_db } if chain.0 == "cmx-di-a" && *gain_db == -60.0
    )));
    assert_eq!(
        chain_di_gain_target(&ChainId("cmx-di-a".into())),
        strip_linear_gain(-60.0, false)
    );
    assert_eq!(chain_di_gain_target(&ChainId("cmx-di-b".into())), 1.0);
    assert_eq!(mix_of(&d, "cmx-di-a").di_gain_db, -60.0);
}
