//! #14/#127 — the click is started by the DISPATCHER, from any transport.
//!
//! The bug: `Event::MetronomeEnabledChanged` was applied only in the GUI's
//! `metronome_events::apply_events`, reachable only from a knob callback. The
//! MCP poll drain and the MIDI drain both land in
//! `chain_rig_nav_wiring::apply_events_to_ui`, which has no metronome handling
//! at all — while `adapter-midi`'s `toggle_metronome` slot dispatches
//! `SetMetronomeEnabled`. So a footswitch press flipped the mirror in
//! `SelectionState`, produced its event, and the click never played.
//!
//! These tests never touch the GUI: they dispatch the command the way MCP and
//! MIDI do and assert on the audio runtime the frontend hosts.
//!
//! #808 rides along: the click is an independent pipeline (invariant #4) and
//! must sound with NO chain enabled, so its start — like the DI's arm — is the
//! one metronome door allowed to create the controller.
//!
//! These dispatch commands that PERSIST in the real app. They cannot reach the
//! machine's `config.yaml` here: `state::metronome_config_path` is `None` in a
//! test build, so the state has nowhere to write (#701).
//!
//! **Half of them belong to the real-hardware battery** (`OPENRIG_HW_TESTS=1`,
//! #670 / `docs/testing.md`). Reaching `start_metronome` at all means
//! `find_output_device_by_id` → `host.output_devices()`, i.e. enumerating the
//! machine's audio interfaces — which this repo has already watched freeze the
//! owner's interface mid-session. A plain `cargo test` must not do it, so the
//! tests that open (or fail to open) a stream are gated, and the ORDER those
//! doors write the generator's flag in is pinned headless next door, in
//! `runtime_pipelines_tests.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, MetronomeCommand};
use domain::ids::{ChainId, DeviceId};
use infra_cpal::ProjectRuntimeController;
use infra_filesystem::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;

use crate::gui_live_source::metronome_live_source;
use crate::runtime_lifecycle::attach_runtime_control;
use crate::state::ProjectSession;

/// The real-hardware gate (#670). Loud skip, never a failure, exactly like the
/// `infra-cpal` battery.
fn hw_enabled() -> bool {
    if std::env::var_os("OPENRIG_HW_TESTS").is_some() {
        return true;
    }
    eprintln!(
        "[#127 metronome HW] SKIPPED — this test reaches the audio host and enumerates the \
         machine's output devices. Run with OPENRIG_HW_TESTS=1 on an idle machine \
         (docs/testing.md → Real-hardware battery)."
    );
    false
}

/// A project opened and left alone: one chain, DISABLED, so nothing but the
/// metronome can ever bring a runtime up.
fn session_with_disabled_chain() -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("metronome-14-play".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false, // opened the project, never enabled the chain
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-metronome-14-play"),
    )
}

/// One output endpoint on a device that does not exist. Opening the stream on
/// it fails, which is fine and deliberate: what these tests prove is what the
/// dispatcher ASKS the runtime for, and what it reports once the answer is no.
///
/// No real device is ever touched — but LOOKING for this one still enumerates
/// them, so any test that reaches the open stays behind [`hw_enabled`].
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

/// One output endpoint on the machine's FIRST REAL output device, so the door
/// can be watched actually opening a stream. `None` on a machine with no output
/// device at all. Only ever built behind [`hw_enabled`] — it enumerates.
fn real_output_endpoint() -> Option<Vec<IoBinding>> {
    let device = infra_cpal::list_output_device_descriptors()
        .ok()?
        .into_iter()
        .find(|device| device.channels > 0)?;
    let stereo = device.channels >= 2;
    Some(vec![IoBinding {
        id: "io-hw".into(),
        name: device.name.clone(),
        inputs: vec![],
        outputs: vec![IoEndpoint {
            name: "Main Out".into(),
            device_id: DeviceId(device.id.clone()),
            mode: if stereo {
                ChannelMode::Stereo
            } else {
                ChannelMode::Mono
            },
            channels: if stereo { vec![0, 1] } else { vec![0] },
        }],
    }])
}

fn stopped_runtime() -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    Rc::new(RefCell::new(None))
}

/// Silence the click before it is ever started: this opens the machine's real
/// output, and a test has no business making a sound on the owner's monitors.
fn silence_the_click(session: &ProjectSession) {
    session
        .dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeVolume {
            volume: 0.0,
        }))
        .expect("setting the volume is never an error");
}

/// The door has to OPEN A STREAM, not just flip a flag. `metronome_active()` is
/// true only with a real `cpal::Stream` parked in the controller, so deleting
/// the `controller.start_metronome(...)` call in `runtime_pipelines` fails this
/// test — which is what makes it worth running.
#[test]
fn enabling_the_metronome_off_the_gui_starts_the_click() {
    if !hw_enabled() {
        return;
    }
    let Some(bindings) = real_output_endpoint() else {
        eprintln!("[#127 metronome HW] SKIPPED — this machine reports no output device.");
        return;
    };
    let project_runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = bindings;
    attach_runtime_control(&project_runtime, &session);
    silence_the_click(&session);

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: no chain enabled, so no controller exists yet"
    );

    // Exactly what the MIDI footswitch slot and the MCP tool dispatch.
    session
        .dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
            enabled: true,
        }))
        .expect("#127: the click must open on a real output device");

    let borrow = project_runtime.borrow();
    let runtime = borrow.as_ref().expect(
        "#808: starting the click must create the runtime controller even with no chain \
         enabled — otherwise POWER opens no stream and the beat lamp freezes on beat one",
    );
    assert!(
        runtime.metronome_active(),
        "#127: the start door must OPEN the click's own output stream — a toggle that \
         only flips the generator's flag makes no sound"
    );
    assert!(
        runtime.metronome_shared().enabled(),
        "#127: a toggle that never reached the GUI must still start the click — a MIDI \
         footswitch or an MCP client flipped the mirror and heard nothing"
    );
}

#[test]
fn disabling_the_metronome_off_the_gui_silences_the_click() {
    if !hw_enabled() {
        return;
    }
    let Some(bindings) = real_output_endpoint() else {
        eprintln!("[#127 metronome HW] SKIPPED — this machine reports no output device.");
        return;
    };
    let project_runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = bindings;
    attach_runtime_control(&project_runtime, &session);
    silence_the_click(&session);
    session
        .dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
            enabled: true,
        }))
        .expect("the click opened on a real output device");
    assert!(
        project_runtime
            .borrow()
            .as_ref()
            .expect("the start created the controller")
            .metronome_active(),
        "precondition: there is a real open stream to silence"
    );

    session
        .dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
            enabled: false,
        }))
        .expect("stopping the click is never an error");

    let borrow = project_runtime.borrow();
    let runtime = borrow.as_ref().expect("the controller is still there");
    assert!(
        !runtime.metronome_active(),
        "the stop has to CLOSE the stream, not just mute it"
    );
    assert!(
        !runtime.metronome_shared().enabled(),
        "the stop has to reach the generator too, or the click keeps sounding"
    );
}

/// The failure the whole task exists to close, end to end: the endpoint
/// resolves, the device behind it does not (gone, busy, renamed), so the stream
/// never opens.
///
/// Read back through the SAME seam the beat lamps and `openrig://metronome`
/// read — `LiveSource::metronome`. Before the fix the door set the generator's
/// `enabled` flag before asking for the stream, so the dispatcher recorded
/// `running=false` on the `Err`, `wire_power` drew POWER off, and 33 ms later
/// the lamp timer lit it again from this very flag: a switch on over silence,
/// with `running: true` over MCP, until the app was restarted.
#[test]
fn a_start_the_device_refused_leaves_the_click_reported_stopped() {
    if !hw_enabled() {
        return;
    }
    let project_runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = one_output_endpoint();
    attach_runtime_control(&project_runtime, &session);

    let result =
        session
            .dispatcher
            .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
                enabled: true,
            }));

    assert!(
        result.is_err(),
        "a stream that never opened must reach the caller as a reason: {result:?}"
    );
    let reading = metronome_live_source(&project_runtime)
        .metronome()
        .expect("the start created the controller, so there is a live reading");
    assert!(
        !reading.running,
        "#127: a click whose stream the device refused must not read as running — this is \
         the flag the 33 ms lamp timer and `openrig://metronome` answer with"
    );
    assert!(
        !project_runtime
            .borrow()
            .as_ref()
            .expect("the controller is there")
            .metronome_active(),
        "and no stream may be left parked for a click that never opened"
    );
}

/// The other half of the #808 rule: only a START may wake audio. A settings
/// edit, a tap or an output pick on a stopped rig must open nothing.
#[test]
fn no_other_metronome_command_starts_the_audio_runtime() {
    let project_runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = one_output_endpoint();
    attach_runtime_control(&project_runtime, &session);

    for command in [
        MetronomeCommand::SetMetronomeBpm { bpm: 90.0 },
        MetronomeCommand::SetMetronomeTimeSignature { beats_per_bar: 3 },
        MetronomeCommand::SetMetronomeVolume { volume: 0.5 },
        MetronomeCommand::SetMetronomeCountIn { enabled: true },
        MetronomeCommand::MetronomeTap,
        MetronomeCommand::SetMetronomeOutput {
            device_id: Some("io-1\u{1f}Main Out".into()),
        },
        MetronomeCommand::SetMetronomeEnabled { enabled: false },
    ] {
        session
            .dispatcher
            .dispatch(Command::Metronome(command))
            .expect("no metronome command fails on a stopped rig");
    }

    assert!(
        project_runtime.borrow().is_none(),
        "only pressing POWER asks to hear something — nothing else may open a device"
    );
}

/// With no output endpoint there is nothing to play through. The failure has
/// to reach the caller (an MCP client gets a reason, not a silent switch) and
/// no audio may be woken for a click that cannot sound.
#[test]
fn starting_with_no_output_endpoint_fails_and_opens_nothing() {
    let project_runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    attach_runtime_control(&project_runtime, &session);

    let result =
        session
            .dispatcher
            .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
                enabled: true,
            }));

    assert!(
        result.is_err(),
        "a click with nowhere to play must say so: {result:?}"
    );
    assert!(
        project_runtime.borrow().is_none(),
        "and it must not open the audio host for a click that cannot sound"
    );
}
