//! #1007: SOLO on the global mixer — a soloed strip silences the strips of
//! its own group that are not soloed, solos add up, clearing them restores,
//! and the stored fader is never touched.
//!
//! The engine's endpoint table is process-global, so every test uses its OWN
//! device id.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::mixer_gain::strip_linear_gain;
use domain::mixer_strip::MixerDirection;
use engine::mixer_gains::endpoint_gain_target;
use infra_filesystem::{FilesystemStorage, MixerStripConfig};
use project::project::Project;

use crate::command::{Command, MixerCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::mixer_state::MixerControlState;

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })))
}

fn ep(device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: format!("{device} {channels:?}"),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

/// Two inputs and three outputs on `device`.
fn rig(device: &str) -> LocalDispatcher {
    let d = dispatcher();
    d.attach_io_bindings(Rc::new(RefCell::new(vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![ep(device, vec![0]), ep(device, vec![1])],
        outputs: vec![
            ep(device, vec![0, 1]),
            ep(device, vec![2, 3]),
            ep(device, vec![4, 5]),
        ],
    }])));
    d
}

fn solo(d: &LocalDispatcher, strip: &str, soloed: bool) -> Vec<Event> {
    d.dispatch(Command::Mixer(MixerCommand::SetMixerSolo {
        strip: strip.into(),
        soloed,
    }))
    .expect("SetMixerSolo dispatches")
}

fn out(device: &str, channels: &[usize]) -> f32 {
    endpoint_gain_target(MixerDirection::Output, device, channels)
}

fn inp(device: &str, channels: &[usize]) -> f32 {
    endpoint_gain_target(MixerDirection::Input, device, channels)
}

#[test]
fn soloing_an_output_silences_the_other_outputs() {
    let d = rig("mxs-one");
    solo(&d, "out:2,3@mxs-one", true);
    assert_eq!(out("mxs-one", &[2, 3]), 1.0, "the soloed output plays");
    assert_eq!(out("mxs-one", &[0, 1]), 0.0);
    assert_eq!(out("mxs-one", &[4, 5]), 0.0);
}

#[test]
fn an_output_solo_leaves_the_inputs_alone() {
    let d = rig("mxs-group");
    solo(&d, "out:0,1@mxs-group", true);
    assert_eq!(inp("mxs-group", &[0]), 1.0);
    assert_eq!(inp("mxs-group", &[1]), 1.0);
}

#[test]
fn soloing_an_input_silences_only_the_other_inputs() {
    let d = rig("mxs-in");
    solo(&d, "in:1@mxs-in", true);
    assert_eq!(inp("mxs-in", &[0]), 0.0);
    assert_eq!(inp("mxs-in", &[1]), 1.0);
    assert_eq!(out("mxs-in", &[0, 1]), 1.0, "outputs are another group");
}

#[test]
fn solos_add_up() {
    let d = rig("mxs-add");
    solo(&d, "out:0,1@mxs-add", true);
    solo(&d, "out:4,5@mxs-add", true);
    assert_eq!(out("mxs-add", &[0, 1]), 1.0);
    assert_eq!(out("mxs-add", &[2, 3]), 0.0);
    assert_eq!(out("mxs-add", &[4, 5]), 1.0);
}

#[test]
fn clearing_every_solo_restores_every_strip() {
    let d = rig("mxs-clear");
    solo(&d, "out:0,1@mxs-clear", true);
    solo(&d, "out:0,1@mxs-clear", false);
    for channels in [&[0, 1][..], &[2, 3], &[4, 5]] {
        assert_eq!(out("mxs-clear", channels), 1.0, "{channels:?}");
    }
}

#[test]
fn toggle_flips_the_solo_and_reports_it() {
    let d = rig("mxs-toggle");
    let toggle = || {
        d.dispatch(Command::Mixer(MixerCommand::ToggleMixerSolo {
            strip: "out:2,3@mxs-toggle".into(),
        }))
        .expect("ToggleMixerSolo dispatches")
        .into_iter()
        .find_map(|e| match e {
            Event::MixerStripChanged { soloed, .. } => Some(soloed),
            _ => None,
        })
        .expect("expected Event::MixerStripChanged")
    };
    assert!(toggle(), "first toggle solos");
    assert_eq!(out("mxs-toggle", &[0, 1]), 0.0);
    assert!(!toggle(), "second toggle clears");
    assert_eq!(out("mxs-toggle", &[0, 1]), 1.0);
}

#[test]
fn solo_never_touches_the_stored_fader() {
    let d = rig("mxs-fader");
    d.dispatch(Command::Mixer(MixerCommand::SetMixerFader {
        strip: "out:0,1@mxs-fader".into(),
        gain_db: -9.0,
    }))
    .expect("fader");
    solo(&d, "out:2,3@mxs-fader", true);
    solo(&d, "out:2,3@mxs-fader", false);
    let strip = d
        .mixer_strips()
        .into_iter()
        .find(|s| s.id == "out:0,1@mxs-fader")
        .expect("strip listed");
    assert_eq!(strip.gain_db, -9.0);
    assert!(
        (out("mxs-fader", &[0, 1]) - strip_linear_gain(-9.0, false)).abs() < 1e-6,
        "the fader is back once the solo clears"
    );
}

#[test]
fn a_fader_moved_while_another_strip_is_soloed_stays_silent() {
    let d = rig("mxs-move");
    solo(&d, "out:0,1@mxs-move", true);
    d.dispatch(Command::Mixer(MixerCommand::SetMixerFader {
        strip: "out:2,3@mxs-move".into(),
        gain_db: -3.0,
    }))
    .expect("fader");
    assert_eq!(out("mxs-move", &[2, 3]), 0.0);
}

#[test]
fn the_strip_list_shows_the_solo() {
    let d = rig("mxs-view");
    solo(&d, "in:0@mxs-view", true);
    let soloed: Vec<(String, bool)> = d
        .mixer_strips()
        .into_iter()
        .map(|s| (s.id, s.soloed))
        .filter(|(id, _)| id.starts_with("in:"))
        .collect();
    assert_eq!(
        soloed,
        vec![
            ("in:0@mxs-view".to_string(), true),
            ("in:1@mxs-view".to_string(), false),
        ]
    );
}

#[test]
fn a_restored_solo_silences_the_rest_of_its_group() {
    let d = rig("mxs-restore");
    d.attach_mixer_state(Rc::new(RefCell::new(MixerControlState::restored(
        &[MixerStripConfig {
            id: "out:4,5@mxs-restore".into(),
            gain_db: 0.0,
            muted: false,
            soloed: true,
        }],
        None,
    ))));
    assert_eq!(out("mxs-restore", &[0, 1]), 0.0);
    assert_eq!(out("mxs-restore", &[4, 5]), 1.0);
}

#[test]
fn a_solo_is_persisted_only_to_the_attached_config() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");
    let d = rig("mxs-persist");
    d.attach_mixer_state(Rc::new(RefCell::new(MixerControlState::restored(
        &[],
        Some(config.clone()),
    ))));
    solo(&d, "out:2,3@mxs-persist", true);
    crate::persist_worker::flush();
    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload");
    assert_eq!(
        reloaded.mixer,
        vec![MixerStripConfig {
            id: "out:2,3@mxs-persist".into(),
            gain_db: 0.0,
            muted: false,
            soloed: true,
        }]
    );
}

#[test]
fn the_mixer_read_carries_the_solo() {
    let d = rig("mxs-read");
    solo(&d, "out:0,1@mxs-read", true);
    let json: serde_json::Value =
        serde_json::from_str(&crate::query_mixer::mixer_state_json(&d.mixer_strips()))
            .expect("json");
    let soloed: Vec<bool> = json["strips"]
        .as_array()
        .expect("strips")
        .iter()
        .map(|s| s["soloed"].as_bool().expect("soloed is a bool"))
        .collect();
    assert_eq!(soloed, vec![false, false, true, false, false]);
}
