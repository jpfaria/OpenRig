//! Red-first (#14): the metronome commands dispatch, emit their events, and —
//! the part that matters — clamp their payloads.
//!
//! Every one of these arrives from MCP, gRPC or a MIDI CC as well as from the
//! GUI, so an out-of-range value is not hypothetical. The clamp lives in the
//! dispatcher precisely so no caller can push 10 000 BPM at the audio thread.

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use crate::command::{Command, MetronomeCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })))
}

fn bpm_after(requested: f32) -> f32 {
    let events = dispatcher()
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeBpm {
            bpm: requested,
        }))
        .expect("SetMetronomeBpm dispatches");
    events
        .iter()
        .find_map(|e| match e {
            Event::MetronomeBpmChanged { bpm } => Some(*bpm),
            _ => None,
        })
        .expect("expected Event::MetronomeBpmChanged")
}

#[test]
fn bpm_is_clamped_to_the_supported_range() {
    assert_eq!(bpm_after(1.0), 30.0, "below range must clamp to BPM_MIN");
    assert_eq!(
        bpm_after(10_000.0),
        300.0,
        "above range must clamp to BPM_MAX"
    );
    assert_eq!(bpm_after(120.0), 120.0, "in-range value passes through");
}

#[test]
fn volume_is_clamped_to_unit_range() {
    let volume_after = |requested: f32| {
        let events = dispatcher()
            .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeVolume {
                volume: requested,
            }))
            .expect("SetMetronomeVolume dispatches");
        events
            .iter()
            .find_map(|e| match e {
                Event::MetronomeVolumeChanged { volume } => Some(*volume),
                _ => None,
            })
            .expect("expected Event::MetronomeVolumeChanged")
    };
    assert_eq!(volume_after(-3.0), 0.0);
    assert_eq!(volume_after(9.0), 1.0);
    assert_eq!(volume_after(0.5), 0.5);
}

#[test]
fn beats_per_bar_is_clamped_to_a_playable_range() {
    let beats_after = |requested: u32| {
        let events = dispatcher()
            .dispatch(Command::Metronome(
                MetronomeCommand::SetMetronomeTimeSignature {
                    beats_per_bar: requested,
                },
            ))
            .expect("SetMetronomeTimeSignature dispatches");
        events
            .iter()
            .find_map(|e| match e {
                Event::MetronomeTimeSignatureChanged { beats_per_bar } => Some(*beats_per_bar),
                _ => None,
            })
            .expect("expected Event::MetronomeTimeSignatureChanged")
    };
    assert_eq!(beats_after(0), 1, "a bar of zero beats has no meaning");
    assert_eq!(beats_after(99), 16, "the beat lamps stop at sixteen");
    assert_eq!(beats_after(7), 7);
}

#[test]
fn enabling_mirrors_into_selection_state() {
    let dispatcher = dispatcher();
    dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
            enabled: true,
        }))
        .expect("SetMetronomeEnabled dispatches");

    let state = dispatcher.selection_state();
    let state = state.read().expect("selection state readable");
    assert!(
        state.metronome_enabled,
        "the MIDI slot toggle_metronome reads this snapshot to know what to flip"
    );
}

#[test]
fn each_metronome_command_emits_its_event() {
    let cases: Vec<(Command, fn(&Event) -> bool)> = vec![
        (
            Command::Metronome(MetronomeCommand::SetMetronomeEnabled { enabled: true }),
            |e| matches!(e, Event::MetronomeEnabledChanged { enabled: true }),
        ),
        (
            Command::Metronome(MetronomeCommand::SetMetronomeSubdivision {
                subdivision: "triplets".into(),
            }),
            |e| matches!(e, Event::MetronomeSubdivisionChanged { .. }),
        ),
        (
            Command::Metronome(MetronomeCommand::SetMetronomeTimbre {
                timbre: "wood".into(),
            }),
            |e| matches!(e, Event::MetronomeTimbreChanged { .. }),
        ),
        (
            Command::Metronome(MetronomeCommand::SetMetronomeCountIn { enabled: true }),
            |e| matches!(e, Event::MetronomeCountInChanged { enabled: true }),
        ),
        (
            Command::Metronome(MetronomeCommand::SetMetronomeOutput {
                device_id: Some("dev-1".into()),
            }),
            |e| matches!(e, Event::MetronomeOutputChanged { .. }),
        ),
        (
            Command::Metronome(MetronomeCommand::MetronomeTap),
            |e| matches!(e, Event::MetronomeTapped),
        ),
    ];

    for (command, matches_event) in cases {
        let label = format!("{command:?}");
        let events = dispatcher()
            .dispatch(command)
            .unwrap_or_else(|e| panic!("{label} should dispatch: {e}"));
        assert!(
            events.iter().any(matches_event),
            "{label} did not emit its event, got {events:?}"
        );
    }
}

/// An unknown enum string must not silently become a different sound.
#[test]
fn an_unknown_subdivision_is_rejected() {
    let result = dispatcher().dispatch(Command::Metronome(
        MetronomeCommand::SetMetronomeSubdivision {
            subdivision: "quintuplets".into(),
        },
    ));
    assert!(
        result.is_err(),
        "an unrecognized subdivision must be an error, not a silent fallback"
    );
}
