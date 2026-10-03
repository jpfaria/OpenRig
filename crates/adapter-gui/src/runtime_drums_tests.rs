//! The drum machine is reached through the dispatcher, from any transport.
//!
//! These dispatch the commands MCP and MIDI send and assert on the audio
//! runtime this frontend hosts. Only PLAY may wake audio; every other drum
//! command on a stopped rig opens nothing.
//!
//! The tests that open (or fail to open) a stream enumerate the machine's
//! audio devices, so they belong to the real-hardware battery
//! (`OPENRIG_HW_TESTS=1`, `docs/testing.md`). Nothing here persists: a test
//! build gives the drum state no config path.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use application::command::{Command, DrumsCommand};
use domain::ids::{ChainId, DeviceId};
use infra_cpal::ProjectRuntimeController;
use infra_filesystem::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;

use crate::runtime_lifecycle::attach_runtime_control;
use crate::state::ProjectSession;

fn hw_enabled() -> bool {
    if std::env::var_os("OPENRIG_HW_TESTS").is_some() {
        return true;
    }
    eprintln!(
        "[drums HW] SKIPPED — this test enumerates the machine's output devices. \
         Run with OPENRIG_HW_TESTS=1 on an idle machine (docs/testing.md)."
    );
    false
}

fn session_with_disabled_chain() -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("drums-play".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
            disabled_endpoints: Default::default(),
            mix: Default::default(),
        }],
        midi: None,
    };
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-drums-play"),
    )
}

fn one_output_endpoint() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io-1".into(),
        name: "Test Interface".into(),
        inputs: vec![],
        outputs: vec![IoEndpoint {
            name: "Main Out".into(),
            device_id: DeviceId("openrig-test-no-such-device".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn real_output_endpoint() -> Option<Vec<IoBinding>> {
    let device = infra_cpal::list_output_device_descriptors()
        .ok()?
        .into_iter()
        .find(|device| device.channels >= 2)?;
    Some(vec![IoBinding {
        id: "io-hw".into(),
        name: device.name.clone(),
        inputs: vec![],
        outputs: vec![IoEndpoint {
            name: "Main Out".into(),
            device_id: DeviceId(device.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }])
}

fn stopped_runtime() -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    Rc::new(RefCell::new(None))
}

fn attach(runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>, session: &ProjectSession) {
    attach_runtime_control(
        runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        session,
    );
}

fn drums(session: &ProjectSession, command: DrumsCommand) -> anyhow::Result<()> {
    session
        .dispatcher
        .dispatch(Command::Drums(command))
        .map(|_| ())
}

/// The bundled folder resolves against the data root (the working directory
/// in a dev build), so the expectation is the same scan, not a fixed count.
#[test]
fn every_session_knows_the_bundled_and_user_drums() {
    use application::drums::{bundled_drum_dir, scan_drum_library, user_drum_dir};
    let expected = scan_drum_library(&[bundled_drum_dir(), user_drum_dir()]);
    let session = session_with_disabled_chain();

    let library = session.dispatcher.drums_library();
    let ids = |kits: &[application::drums::DrumKitEntry]| {
        kits.iter().map(|k| k.id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(ids(&library.kits), ids(&expected.kits));
    assert_eq!(library.grooves.len(), expected.grooves.len());
    let snapshot = session.dispatcher.drums_snapshot();
    assert_eq!(
        snapshot.kit.is_some(),
        !expected.kits.is_empty(),
        "a session always starts on a kit when there is one"
    );
}

#[test]
fn no_drum_command_but_play_starts_the_audio_runtime() {
    let runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = one_output_endpoint();
    attach(&runtime, &session);

    for command in [
        DrumsCommand::SetDrumsBpm { bpm: 90.0 },
        DrumsCommand::SetDrumsVolume { volume: 0.5 },
        DrumsCommand::SetDrumsOutput {
            output_key: Some("io-1\u{1f}Main Out".into()),
        },
        DrumsCommand::StopDrums,
        DrumsCommand::SetDrumsEnabled { enabled: false },
    ] {
        drums(&session, command).expect("no drum command fails on a stopped rig");
    }

    assert!(
        runtime.borrow().is_none(),
        "only PLAY asks to hear the drums — nothing else may open a device"
    );
}

#[test]
fn playing_with_no_output_endpoint_fails_and_opens_nothing() {
    let runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    attach(&runtime, &session);

    let result = drums(&session, DrumsCommand::PlayDrums);

    assert!(
        result.is_err(),
        "with nothing to play through, PLAY must say so instead of looking on"
    );
    assert!(runtime.borrow().is_none(), "and no audio may be woken");
    assert!(!session.dispatcher.drums_snapshot().playing);
}

#[test]
fn playing_opens_the_drums_own_stream_with_no_chain_enabled() {
    if !hw_enabled() {
        return;
    }
    let Some(bindings) = real_output_endpoint() else {
        eprintln!("[drums HW] SKIPPED — no stereo output device on this machine");
        return;
    };
    let runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = bindings;
    attach(&runtime, &session);
    // Silent on the owner's monitors: the stream runs, the kit is quiet.
    drums(&session, DrumsCommand::SetDrumsVolume { volume: 0.0 }).unwrap();

    drums(&session, DrumsCommand::PlayDrums).expect("the drums open on a real output");

    let shared = {
        let borrow = runtime.borrow();
        let controller = borrow.as_ref().expect("PLAY creates the controller");
        assert!(controller.drums_active(), "the drums have their own stream");
        controller.drums_shared()
    };
    assert!(shared.enabled() && shared.playing());
    // `cargo test` runs from the crate folder, where the bundled library is
    // not found; hand the kit to the door directly.
    let kit_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/drums/kits/black-pearl");
    let control = crate::runtime_lifecycle::GuiRuntimeControl {
        runtime: Rc::clone(&runtime),
        analyzers: crate::runtime_analyzers::AnalyzerSessions::detached(),
        session: crate::runtime_session_handle::SessionHandle::mirror(&session),
        drum_kit: Default::default(),
    };
    application::drums_runtime::DrumsRuntime::set_drum_kit(&control, &kit_dir);
    let deadline = Instant::now() + Duration::from_secs(5);
    while shared
        .kits()
        .latest()
        .map_or(true, |kit| kit.name().is_empty())
    {
        assert!(
            Instant::now() < deadline,
            "the kit loads off the control thread"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        crate::gui_live_source::drums_position(&runtime).is_some_and(|p| p.playing),
        "the live position reaches the read seam"
    );

    drums(&session, DrumsCommand::SetDrumsEnabled { enabled: false }).unwrap();
    let borrow = runtime.borrow();
    assert!(
        !borrow.as_ref().unwrap().drums_active(),
        "OFF closes the stream"
    );
}
