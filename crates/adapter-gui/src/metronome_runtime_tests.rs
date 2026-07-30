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

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, MetronomeCommand};
use domain::ids::{ChainId, DeviceId};
use infra_cpal::ProjectRuntimeController;
use infra_filesystem::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;

use crate::runtime_lifecycle::attach_runtime_control;
use crate::state::ProjectSession;

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
/// dispatcher ASKS the runtime for, with no real device anywhere near them.
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

fn stopped_runtime() -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    Rc::new(RefCell::new(None))
}

#[test]
fn enabling_the_metronome_off_the_gui_starts_the_click() {
    let project_runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = one_output_endpoint();
    attach_runtime_control(&project_runtime, &session);

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: no chain enabled, so no controller exists yet"
    );

    // Exactly what the MIDI footswitch slot and the MCP tool dispatch. Opening
    // the stream on a device that does not exist fails; the runtime being
    // asked at all is what was missing.
    let _ =
        session
            .dispatcher
            .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
                enabled: true,
            }));

    let borrow = project_runtime.borrow();
    let runtime = borrow.as_ref().expect(
        "#808: starting the click must create the runtime controller even with no chain \
         enabled — otherwise POWER opens no stream and the beat lamp freezes on beat one",
    );
    assert!(
        runtime.metronome_shared().enabled(),
        "#127: a toggle that never reached the GUI must still start the click — a MIDI \
         footswitch or an MCP client flipped the mirror and heard nothing"
    );
}

#[test]
fn disabling_the_metronome_off_the_gui_silences_the_click() {
    let project_runtime = stopped_runtime();
    let session = session_with_disabled_chain();
    *session.io_bindings.borrow_mut() = one_output_endpoint();
    attach_runtime_control(&project_runtime, &session);
    let _ =
        session
            .dispatcher
            .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
                enabled: true,
            }));

    session
        .dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
            enabled: false,
        }))
        .expect("stopping the click is never an error");

    assert!(
        !project_runtime
            .borrow()
            .as_ref()
            .expect("the controller is still there")
            .metronome_shared()
            .enabled(),
        "the stop has to reach the generator too, or the click keeps sounding"
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
