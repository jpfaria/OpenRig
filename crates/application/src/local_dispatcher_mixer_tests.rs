//! #1007: the global mixer commands — validated, remembered, pushed to the
//! engine's per-endpoint gain, persisted and reported.
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

fn ep(device: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: format!("{device} {channels:?}"),
        device_id: DeviceId(device.into()),
        mode,
        channels,
    }
}

fn with_bindings(d: &LocalDispatcher, inputs: Vec<IoEndpoint>, outputs: Vec<IoEndpoint>) {
    d.attach_io_bindings(Rc::new(RefCell::new(vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs,
        outputs,
    }])));
}

fn fader(d: &LocalDispatcher, strip: &str, gain_db: f32) -> Vec<Event> {
    d.dispatch(Command::Mixer(MixerCommand::SetMixerFader {
        strip: strip.into(),
        gain_db,
    }))
    .expect("SetMixerFader dispatches")
}

fn changed(events: &[Event]) -> (String, f32, bool) {
    events
        .iter()
        .find_map(|e| match e {
            Event::MixerStripChanged {
                strip,
                gain_db,
                muted,
                ..
            } => Some((strip.clone(), *gain_db, *muted)),
            _ => None,
        })
        .expect("expected Event::MixerStripChanged")
}

#[test]
fn a_fader_move_reaches_the_engine_and_is_reported() {
    let d = dispatcher();
    let events = fader(&d, "out:0,1@mxd-fader", -6.0);
    assert_eq!(changed(&events), ("out:0,1@mxd-fader".into(), -6.0, false));
    let target = endpoint_gain_target(MixerDirection::Output, "mxd-fader", &[0, 1]);
    assert!(
        (target - strip_linear_gain(-6.0, false)).abs() < 1e-6,
        "{target}"
    );
}

#[test]
fn the_fader_is_clamped_to_its_range() {
    let d = dispatcher();
    assert_eq!(changed(&fader(&d, "out:0@mxd-clamp", 99.0)).1, 12.0);
    assert_eq!(changed(&fader(&d, "out:0@mxd-clamp", -999.0)).1, -60.0);
}

#[test]
fn a_garbage_strip_id_is_rejected() {
    let d = dispatcher();
    let result = d.dispatch(Command::Mixer(MixerCommand::SetMixerFader {
        strip: "not-a-strip".into(),
        gain_db: -3.0,
    }));
    assert!(result.is_err(), "an unparseable strip must be an error");
}

#[test]
fn mute_silences_the_endpoint_and_keeps_the_fader() {
    let d = dispatcher();
    fader(&d, "in:3@mxd-mute", -4.0);
    let events = d
        .dispatch(Command::Mixer(MixerCommand::SetMixerMute {
            strip: "in:3@mxd-mute".into(),
            muted: true,
        }))
        .expect("SetMixerMute dispatches");
    assert_eq!(changed(&events), ("in:3@mxd-mute".into(), -4.0, true));
    assert_eq!(
        endpoint_gain_target(MixerDirection::Input, "mxd-mute", &[3]),
        0.0
    );
}

#[test]
fn toggle_flips_the_mute() {
    let d = dispatcher();
    let toggle = || {
        changed(
            &d.dispatch(Command::Mixer(MixerCommand::ToggleMixerMute {
                strip: "out:4,5@mxd-toggle".into(),
            }))
            .expect("ToggleMixerMute dispatches"),
        )
        .2
    };
    assert!(toggle(), "first toggle mutes");
    assert!(!toggle(), "second toggle unmutes");
    assert_eq!(
        endpoint_gain_target(MixerDirection::Output, "mxd-toggle", &[4, 5]),
        1.0
    );
}

#[test]
fn a_mono_input_strip_reaches_every_split_pipeline() {
    let d = dispatcher();
    with_bindings(
        &d,
        vec![ep("mxd-split", ChannelMode::Mono, vec![0, 1])],
        vec![],
    );
    fader(&d, "in:0,1@mxd-split", -6.0);
    let expected = strip_linear_gain(-6.0, false);
    for channels in [&[0, 1][..], &[0], &[1]] {
        let target = endpoint_gain_target(MixerDirection::Input, "mxd-split", channels);
        assert!(
            (target - expected).abs() < 1e-6,
            "{channels:?} must carry the fader, got {target}"
        );
    }
}

#[test]
fn installing_new_bindings_reapplies_the_fan_out() {
    let d = dispatcher();
    fader(&d, "in:0,1@mxd-rebind", -6.0);
    with_bindings(
        &d,
        vec![ep("mxd-rebind", ChannelMode::Mono, vec![0, 1])],
        vec![],
    );
    d.dispatch(Command::IoBinding(
        crate::command::IoBindingCommand::SetIoBindings,
    ))
    .expect("SetIoBindings dispatches");
    let target = endpoint_gain_target(MixerDirection::Input, "mxd-rebind", &[1]);
    assert!(
        (target - strip_linear_gain(-6.0, false)).abs() < 1e-6,
        "the split pipeline on channel 1 must carry the fader, got {target}"
    );
}

#[test]
fn the_strip_list_comes_from_the_bindings_with_their_settings() {
    let d = dispatcher();
    with_bindings(
        &d,
        vec![ep("mxd-list", ChannelMode::Mono, vec![0])],
        vec![ep("mxd-list", ChannelMode::Stereo, vec![0, 1])],
    );
    fader(&d, "out:0,1@mxd-list", -9.0);
    let strips = d.mixer_strips();
    let summary: Vec<(String, f32, bool)> = strips
        .iter()
        .map(|s| (s.id.clone(), s.gain_db, s.muted))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("in:0@mxd-list".to_string(), 0.0, false),
            ("out:0,1@mxd-list".to_string(), -9.0, false),
        ]
    );
}

#[test]
fn attaching_restored_settings_applies_them_to_the_engine() {
    let d = dispatcher();
    let restored = MixerControlState::restored(
        &[MixerStripConfig {
            id: "out:0,1@mxd-restore".into(),
            gain_db: -12.0,
            muted: false,
            soloed: false,
        }],
        None,
    );
    d.attach_mixer_state(Rc::new(RefCell::new(restored)));
    let target = endpoint_gain_target(MixerDirection::Output, "mxd-restore", &[0, 1]);
    assert!(
        (target - strip_linear_gain(-12.0, false)).abs() < 1e-6,
        "{target}"
    );
}

#[test]
fn a_fader_move_is_persisted_only_to_the_attached_config() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");
    let d = dispatcher();
    d.attach_mixer_state(Rc::new(RefCell::new(MixerControlState::restored(
        &[],
        Some(config.clone()),
    ))));
    fader(&d, "out:0,1@mxd-persist", -5.0);
    crate::persist_worker::flush();
    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload");
    assert_eq!(
        reloaded.mixer,
        vec![MixerStripConfig {
            id: "out:0,1@mxd-persist".into(),
            gain_db: -5.0,
            muted: false,
            soloed: false,
        }]
    );
}
