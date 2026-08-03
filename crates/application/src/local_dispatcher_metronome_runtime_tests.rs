//! #127 (Task 12b), red-first: the click is a runtime effect the DISPATCHER
//! applies, not something the GUI does after dispatching.
//!
//! The bug this closes is real and reported by the code itself:
//! `Event::MetronomeEnabledChanged` was applied ONLY in the GUI's own
//! `metronome_events::apply_events`, reached only from a knob callback. The
//! MCP poll drain and the MIDI footswitch drain both land in
//! `chain_rig_nav_wiring::apply_events_to_ui`, which has no metronome handling
//! at all — while `adapter-midi`'s `toggle_metronome` slot happily dispatches
//! `SetMetronomeEnabled`. So a footswitch press (or an MCP client) flipped the
//! dispatcher's mirror, got its event, and the click never played.
//!
//! Every test here drives the DISPATCHER, which is what every non-GUI
//! transport reaches. Nothing in it knows about Slint.

use std::cell::RefCell;
use std::rc::Rc;

use feature_dsp::metronome::{MetronomeSettings, Subdivision, Timbre};
use infra_filesystem::MetronomeConfig;
use project::project::Project;

use crate::command::{Command, MetronomeCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::metronome_state::MetronomeControlState;
use crate::runtime_control::RuntimeControl;

/// What the runtime was asked to do, in order. Each entry names the settings
/// (or the endpoint) it carried, because a door that cannot say WHAT it asked
/// for cannot be proven to have carried the dispatcher's state rather than a
/// stale copy.
#[derive(Default)]
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<String>>>,
}

fn describe(settings: MetronomeSettings) -> String {
    format!(
        "bpm={} beats={} sub={} timbre={} vol={} count_in={}",
        settings.bpm,
        settings.beats_per_bar,
        settings.subdivision.key(),
        settings.timbre.key(),
        settings.volume,
        settings.count_in,
    )
}

impl RuntimeControl for SpyRuntimeControl {
    fn start_metronome(
        &self,
        settings: MetronomeSettings,
        output_key: Option<&str>,
    ) -> anyhow::Result<()> {
        self.calls.borrow_mut().push(format!(
            "start [{}] on {}",
            describe(settings),
            output_key.unwrap_or("<first>")
        ));
        Ok(())
    }

    fn stop_metronome(&self) {
        self.calls.borrow_mut().push("stop".to_string());
    }

    fn set_metronome_settings(&self, settings: MetronomeSettings) {
        self.calls
            .borrow_mut()
            .push(format!("settings [{}]", describe(settings)));
    }

    fn refresh_metronome_output(&self, output_key: Option<&str>) -> anyhow::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("refresh on {}", output_key.unwrap_or("<first>")));
        Ok(())
    }

    /// Isolation (`CLAUDE.md` LAW): the click is its own pipeline. If a
    /// metronome command ever asked a CHAIN's runtime for anything, it shows
    /// up in the same log and the isolation test below fails.
    fn sync_chain(&self, chain: &domain::ids::ChainId) -> anyhow::Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("sync chain {}", chain.0));
        Ok(())
    }

    fn disarm_di_stream(&self, chain: &domain::ids::ChainId) {
        self.calls
            .borrow_mut()
            .push(format!("disarm di {}", chain.0));
    }
}

struct FailingRuntimeControl;

impl RuntimeControl for FailingRuntimeControl {
    fn start_metronome(
        &self,
        _settings: MetronomeSettings,
        _output_key: Option<&str>,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "no project output endpoint to play through"
        ))
    }
}

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })))
}

/// A dispatcher with the spy attached, and the log it writes into.
fn spied() -> (LocalDispatcher, Rc<RefCell<Vec<String>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher();
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));
    (dispatcher, calls)
}

fn cmd(cmd: MetronomeCommand) -> Command {
    Command::Metronome(cmd)
}

/// The default settings, as `SpyRuntimeControl` renders them.
fn default_settings() -> String {
    describe(MetronomeSettings::default())
}

#[test]
fn enabling_the_metronome_starts_the_click_through_the_bus() {
    let (dispatcher, calls) = spied();

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeEnabled { enabled: true }))
        .expect("SetMetronomeEnabled must succeed");

    assert_eq!(
        *calls.borrow(),
        vec![format!("start [{}] on <first>", default_settings())],
        "the dispatcher must start the click itself — otherwise a MIDI \
         footswitch bound to `toggle_metronome`, or an MCP client, flips the \
         mirror and nothing is ever heard"
    );
}

#[test]
fn disabling_the_metronome_stops_the_click_through_the_bus() {
    let (dispatcher, calls) = spied();

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeEnabled {
            enabled: false,
        }))
        .expect("SetMetronomeEnabled must succeed");

    assert_eq!(
        *calls.borrow(),
        vec!["stop".to_string()],
        "stopping the click must reach the runtime from the bus too"
    );
}

#[test]
fn a_tempo_change_reaches_the_click_from_any_transport() {
    let (dispatcher, calls) = spied();

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeBpm { bpm: 90.0 }))
        .expect("SetMetronomeBpm must succeed");

    let expected = describe(MetronomeSettings {
        bpm: 90.0,
        ..MetronomeSettings::default()
    });
    assert_eq!(
        *calls.borrow(),
        vec![format!("settings [{expected}]")],
        "a tempo set over MCP/MIDI must reach the generator, not just the GUI"
    );
}

/// The whole point of moving the state: the click starts with what the
/// DISPATCHER holds, so a transport that never touched the GUI's session still
/// gets the tempo, bar, subdivision, timbre, volume and count-in the user set.
#[test]
fn the_click_starts_with_the_settings_the_dispatcher_accumulated() {
    let (dispatcher, calls) = spied();

    for command in [
        MetronomeCommand::SetMetronomeBpm { bpm: 90.0 },
        MetronomeCommand::SetMetronomeTimeSignature { beats_per_bar: 3 },
        MetronomeCommand::SetMetronomeSubdivision {
            subdivision: "triplets".into(),
        },
        MetronomeCommand::SetMetronomeTimbre {
            timbre: "wood".into(),
        },
        MetronomeCommand::SetMetronomeVolume { volume: 0.25 },
        MetronomeCommand::SetMetronomeCountIn { enabled: true },
    ] {
        dispatcher.dispatch(cmd(command)).expect("must dispatch");
    }
    calls.borrow_mut().clear();

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeEnabled { enabled: true }))
        .expect("SetMetronomeEnabled must succeed");

    let expected = describe(MetronomeSettings {
        bpm: 90.0,
        beats_per_bar: 3,
        subdivision: Subdivision::Triplets,
        timbre: Timbre::Wood,
        volume: 0.25,
        count_in: true,
    });
    assert_eq!(
        *calls.borrow(),
        vec![format!("start [{expected}] on <first>")],
        "the click must open with the settings the dispatcher owns"
    );
}

#[test]
fn the_click_starts_on_the_endpoint_that_was_picked() {
    let (dispatcher, calls) = spied();

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeOutput {
            device_id: Some("binding-a\u{1f}main".into()),
        }))
        .expect("SetMetronomeOutput must succeed");
    calls.borrow_mut().clear();

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeEnabled { enabled: true }))
        .expect("SetMetronomeEnabled must succeed");

    assert_eq!(
        *calls.borrow(),
        vec![format!(
            "start [{}] on binding-a\u{1f}main",
            default_settings()
        )],
        "the endpoint key the user picked must travel with the start"
    );
}

/// Picking an output is not a play: a stopped click stays stopped, a running
/// one follows. Same split as `arm_di_stream` vs `refresh_di_stream`.
#[test]
fn picking_an_output_asks_the_click_to_follow_and_never_to_start() {
    let (dispatcher, calls) = spied();

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeOutput {
            device_id: Some("binding-b\u{1f}phones".into()),
        }))
        .expect("SetMetronomeOutput must succeed");

    assert_eq!(
        *calls.borrow(),
        vec!["refresh on binding-b\u{1f}phones".to_string()],
        "an output pick must move a PLAYING click and start nothing"
    );
}

/// Isolation (`CLAUDE.md` LAW): the metronome is an independent pipeline. No
/// metronome command may touch, sync or group with any chain's runtime.
#[test]
fn no_metronome_command_ever_touches_a_chains_runtime() {
    let (dispatcher, calls) = spied();

    for command in [
        MetronomeCommand::SetMetronomeEnabled { enabled: true },
        MetronomeCommand::SetMetronomeBpm { bpm: 140.0 },
        MetronomeCommand::SetMetronomeOutput {
            device_id: Some("binding-a\u{1f}main".into()),
        },
        MetronomeCommand::SetMetronomeEnabled { enabled: false },
    ] {
        dispatcher.dispatch(cmd(command)).expect("must dispatch");
    }

    let chain_touches: Vec<String> = calls
        .borrow()
        .iter()
        .filter(|call| call.contains("chain") || call.contains("di "))
        .cloned()
        .collect();
    assert!(
        chain_touches.is_empty(),
        "the click owns its own output stream (invariant #4) — it must never \
         reach a chain's runtime: {chain_touches:?}"
    );
}

#[test]
fn a_failing_start_surfaces_as_a_dispatch_error() {
    let dispatcher = dispatcher();
    dispatcher.attach_runtime_control(Rc::new(FailingRuntimeControl));

    let result = dispatcher.dispatch(cmd(MetronomeCommand::SetMetronomeEnabled { enabled: true }));

    assert!(
        result.is_err(),
        "a runtime that cannot open the click's stream must not be swallowed: \
         {result:?}"
    );
}

/// A start that the runtime refused made no sound, so the state must not claim
/// it did. `openrig://metronome` answers `running` from this flag whenever no
/// audio runtime is hosted, and the MIDI `toggle_metronome` slot reads the
/// mirror to know what the next press means — a lie here is a click that reads
/// as playing and a footswitch that turns "off" something that never started.
#[test]
fn a_start_the_runtime_refused_leaves_the_click_marked_stopped() {
    let dispatcher = dispatcher();
    dispatcher.attach_runtime_control(Rc::new(FailingRuntimeControl));

    let _ = dispatcher.dispatch(cmd(MetronomeCommand::SetMetronomeEnabled { enabled: true }));

    assert!(
        !dispatcher.metronome_snapshot().running,
        "a click whose stream never opened must not read as running"
    );
    assert!(
        !dispatcher
            .selection_state()
            .read()
            .expect("selection state")
            .metronome_enabled,
        "and the footswitch mirror must not flip either — the next press has to \
         try starting again, not stopping nothing"
    );
}

/// Tap tempo belongs to the dispatcher for the same reason the rest does: a
/// footswitch bound to the tap is a transport like any other, and the tempo it
/// implies has to reach the click.
#[test]
fn tapping_twice_sets_the_tempo_for_every_transport() {
    let (dispatcher, _calls) = spied();

    let first = dispatcher
        .dispatch(cmd(MetronomeCommand::MetronomeTap))
        .expect("MetronomeTap must succeed");
    assert!(
        !first
            .iter()
            .any(|e| matches!(e, Event::MetronomeBpmChanged { .. })),
        "one tap implies no interval and therefore no tempo, got {first:?}"
    );

    let second = dispatcher
        .dispatch(cmd(MetronomeCommand::MetronomeTap))
        .expect("MetronomeTap must succeed");
    assert!(
        second
            .iter()
            .any(|e| matches!(e, Event::MetronomeBpmChanged { .. })),
        "the second tap must publish the tempo it implies, got {second:?}"
    );
}

/// The persisted per-machine settings are the dispatcher's starting point, so
/// a click started over MCP on a freshly opened app plays the user's tempo and
/// not a hardcoded 120.
#[test]
fn the_persisted_config_seeds_the_dispatchers_settings() {
    let (dispatcher, calls) = spied();
    dispatcher.attach_metronome_state(Rc::new(RefCell::new(MetronomeControlState::restored(
        &MetronomeConfig {
            bpm: 76.0,
            beats_per_bar: 6,
            subdivision: "eighths".into(),
            timbre: "beep".into(),
            volume: 0.4,
            count_in: true,
            output_device: Some("binding-c\u{1f}mon".into()),
        },
        // No config path: a test must never be able to write the user's real
        // `config.yaml` (#701).
        None,
    ))));

    dispatcher
        .dispatch(cmd(MetronomeCommand::SetMetronomeEnabled { enabled: true }))
        .expect("SetMetronomeEnabled must succeed");

    let expected = describe(MetronomeSettings {
        bpm: 76.0,
        beats_per_bar: 6,
        subdivision: Subdivision::Eighths,
        timbre: Timbre::Beep,
        volume: 0.4,
        count_in: true,
    });
    assert_eq!(
        *calls.borrow(),
        vec![format!("start [{expected}] on binding-c\u{1f}mon")],
        "the restored settings must be the ones the click opens with"
    );
}

/// `MetronomeConfig` has no `enabled` field on purpose (the app always boots
/// silent). Restoring must not sound anything by itself either.
#[test]
fn restoring_the_saved_settings_never_starts_the_click() {
    let (dispatcher, calls) = spied();

    dispatcher.attach_metronome_state(Rc::new(RefCell::new(MetronomeControlState::restored(
        &MetronomeConfig::default(),
        None,
    ))));

    assert!(
        calls.borrow().is_empty(),
        "restoring saved settings is not a play: {:?}",
        calls.borrow()
    );
}
